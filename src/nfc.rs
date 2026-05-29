use embassy_time::{Duration, Timer};
use esp_hal::{Async, i2c::master::I2c};

pub use crate::types::{NfcError, NfcTag};
use crate::{
    config::{NFC_2_5ML_TAG_UID, NFC_5ML_TAG_UID, NFC_20ML_TAG_UID, PN532_I2C_ADDR},
    dosing::{FENTANYL_DRUG_INDEX, SYRINGE_2_5ML_INDEX, SYRINGE_5ML_INDEX, SYRINGE_20ML_INDEX},
};

// PN532 frame constants.

const PN532_HOST_TO_PN532: u8 = 0xD4;
const PN532_PN532_TO_HOST: u8 = 0xD5;
const PN532_I2C_READY: u8 = 0x01;
const PN532_COMMAND_SAM_CONFIGURATION: u8 = 0x14;
const PN532_COMMAND_RF_CONFIGURATION: u8 = 0x32;
const PN532_COMMAND_INLIST_PASSIVE_TARGET: u8 = 0x4A;
const PN532_RESPONSE_SAM_CONFIGURATION: u8 = 0x15;
const PN532_RESPONSE_RF_CONFIGURATION: u8 = 0x33;
const PN532_RESPONSE_INLIST_PASSIVE_TARGET: u8 = 0x4B;
const PN532_MIFARE_ISO14443A: u8 = 0x00;
const PN532_MAX_FRAME_LEN: usize = 40;
const PN532_ACK_FRAME: [u8; 6] = [0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00];

// PN532 I2C driver state.

pub struct Pn532 {
    i2c: I2c<'static, Async>,
    initialized: bool,
}

impl Pn532 {
    pub fn new(i2c: I2c<'static, Async>) -> Self {
        Self {
            i2c,
            initialized: false,
        }
    }

    /// Polls for known syringe tags and maps them to setup choices.
    pub async fn poll_known_tag(&mut self) -> Result<Option<NfcTag>, NfcError> {
        if !self.initialized {
            self.wake_and_configure().await?;
            self.initialized = true;
        }

        let mut uid = [0u8; 10];
        let Some(uid_len) = self.read_iso14443a_uid(&mut uid).await? else {
            return Ok(None);
        };

        if uid_len == NFC_2_5ML_TAG_UID.len() && uid[..uid_len] == NFC_2_5ML_TAG_UID {
            Ok(Some(NfcTag::SyringePreset(SYRINGE_2_5ML_INDEX)))
        } else if uid_len == NFC_5ML_TAG_UID.len() && uid[..uid_len] == NFC_5ML_TAG_UID {
            Ok(Some(NfcTag::SyringePreset(SYRINGE_5ML_INDEX)))
        } else if uid_len == NFC_20ML_TAG_UID.len() && uid[..uid_len] == NFC_20ML_TAG_UID {
            Ok(Some(NfcTag::SyringePresetWithDrug {
                syringe_index: SYRINGE_20ML_INDEX,
                drug_index: FENTANYL_DRUG_INDEX,
            }))
        } else {
            Ok(None)
        }
    }

    // Wakes the PN532 and configures passive target polling.
    async fn wake_and_configure(&mut self) -> Result<(), NfcError> {
        let _ = self.i2c.write_async(PN532_I2C_ADDR, &[0x00]).await;
        Timer::after_millis(10).await;

        self.send_command(&[PN532_COMMAND_SAM_CONFIGURATION, 0x01, 0x14, 0x01])
            .await?;

        let mut data = [0u8; 8];
        let len = self.read_response(&mut data).await?;
        if len >= 1 && data[0] == PN532_RESPONSE_SAM_CONFIGURATION {
            self.send_command(&[PN532_COMMAND_RF_CONFIGURATION, 0x05, 0x00, 0x00, 0x01])
                .await?;

            let len = self.read_response(&mut data).await?;
            if len >= 1 && data[0] == PN532_RESPONSE_RF_CONFIGURATION {
                Ok(())
            } else {
                Err(NfcError::Protocol)
            }
        } else {
            Err(NfcError::Protocol)
        }
    }

    // Sends InListPassiveTarget and extracts the ISO14443A UID.
    async fn read_iso14443a_uid(&mut self, uid: &mut [u8; 10]) -> Result<Option<usize>, NfcError> {
        self.send_command(&[
            PN532_COMMAND_INLIST_PASSIVE_TARGET,
            0x01,
            PN532_MIFARE_ISO14443A,
        ])
        .await?;

        let mut data = [0u8; PN532_MAX_FRAME_LEN];
        let len = self.read_response(&mut data).await?;

        if len < 2 || data[0] != PN532_RESPONSE_INLIST_PASSIVE_TARGET || data[1] == 0 {
            return Ok(None);
        }

        if len < 7 {
            return Err(NfcError::Protocol);
        }

        let uid_len = data[6] as usize;
        if uid_len == 0 || uid_len > uid.len() || 7 + uid_len > len {
            return Err(NfcError::Protocol);
        }

        uid[..uid_len].copy_from_slice(&data[7..7 + uid_len]);
        Ok(Some(uid_len))
    }

    // Builds a normal PN532 host frame and waits for ACK.
    async fn send_command(&mut self, data: &[u8]) -> Result<(), NfcError> {
        let mut frame = [0u8; PN532_MAX_FRAME_LEN];
        let len = data.len() + 1;
        if len + 7 > frame.len() {
            return Err(NfcError::Protocol);
        }

        frame[0] = 0x00;
        frame[1] = 0x00;
        frame[2] = 0xFF;
        frame[3] = len as u8;
        frame[4] = (!frame[3]).wrapping_add(1);
        frame[5] = PN532_HOST_TO_PN532;
        frame[6..6 + data.len()].copy_from_slice(data);

        let mut checksum = PN532_HOST_TO_PN532;
        for byte in data {
            checksum = checksum.wrapping_add(*byte);
        }
        frame[6 + data.len()] = (!checksum).wrapping_add(1);
        frame[7 + data.len()] = 0x00;

        self.i2c
            .write_async(PN532_I2C_ADDR, &frame[..8 + data.len()])
            .await?;
        self.read_ack().await
    }

    // Reads and validates the PN532 ACK frame.
    async fn read_ack(&mut self) -> Result<(), NfcError> {
        self.wait_ready().await?;

        let mut response = [0u8; 7];
        self.i2c
            .read_async(PN532_I2C_ADDR, &mut response)
            .await
            .map_err(NfcError::I2c)?;

        if response[0] == PN532_I2C_READY && response[1..] == PN532_ACK_FRAME {
            Ok(())
        } else {
            Err(NfcError::Protocol)
        }
    }

    // Reads a PN532 response frame after the I2C-ready byte.
    async fn read_response(&mut self, data: &mut [u8]) -> Result<usize, NfcError> {
        self.wait_ready().await?;

        let mut response = [0u8; PN532_MAX_FRAME_LEN + 1];
        self.i2c
            .read_async(PN532_I2C_ADDR, &mut response)
            .await
            .map_err(NfcError::I2c)?;

        if response[0] != PN532_I2C_READY {
            return Err(NfcError::NotReady);
        }

        parse_frame(&response[1..], data)
    }

    // Polls the PN532 I2C-ready byte with a short timeout.
    async fn wait_ready(&mut self) -> Result<(), NfcError> {
        for _ in 0..20 {
            let mut status = [0u8; 1];
            if self
                .i2c
                .read_async(PN532_I2C_ADDR, &mut status)
                .await
                .is_ok()
                && status[0] == PN532_I2C_READY
            {
                return Ok(());
            }
            Timer::after(Duration::from_millis(5)).await;
        }

        Err(NfcError::NotReady)
    }
}

// Parses a PN532 frame payload and verifies checksum.
fn parse_frame(frame: &[u8], data: &mut [u8]) -> Result<usize, NfcError> {
    if frame.len() < 8 || frame[0] != 0x00 || frame[1] != 0x00 || frame[2] != 0xFF {
        return Err(NfcError::Protocol);
    }

    let len = frame[3] as usize;
    if len == 0 || frame[3].wrapping_add(frame[4]) != 0 || frame[5] != PN532_PN532_TO_HOST {
        return Err(NfcError::Protocol);
    }

    let data_len = len - 1;
    if data_len > data.len() || 6 + data_len >= frame.len() {
        return Err(NfcError::Protocol);
    }

    let payload = &frame[6..6 + data_len];
    let dcs = frame[6 + data_len];
    let mut checksum = PN532_PN532_TO_HOST;
    for byte in payload {
        checksum = checksum.wrapping_add(*byte);
    }

    if checksum.wrapping_add(dcs) != 0 {
        return Err(NfcError::Protocol);
    }

    data[..data_len].copy_from_slice(payload);
    Ok(data_len)
}
