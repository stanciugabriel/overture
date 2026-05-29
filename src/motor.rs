use core::sync::atomic::{AtomicU32, Ordering};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::{
    Async,
    gpio::{Level, Output},
    rmt::{Channel as RmtChannel, PulseCode, Tx},
};

use crate::config::{
    DISPENSE_RAMP_STEPS, DISPENSE_START_STEP_PERIOD_US, STEP_HIGH_US, TONE_FULL_STEP_SCALE,
};
pub use crate::types::{MotionDirection, MotorRunKind, MotorStatus};

// Shared status construction.

impl MotorStatus {
    const fn idle() -> Self {
        Self {
            command_id: 0,
            completed_command_id: 0,
            running: false,
            kind: MotorRunKind::Idle,
            position_steps: 0,
            command_steps: 0,
            total_steps: 0,
        }
    }
}

// HAL pin ownership and direction control.

pub struct MotorPins {
    pub dir: Output<'static>,
    pub step: RmtChannel<'static, Async, Tx>,
    pub enable: Output<'static>,
}

impl MotorPins {
    fn disable(&mut self) {
        self.enable.set_high();
    }

    fn enable(&mut self) {
        self.enable.set_low();
    }

    fn set_motion_direction(&mut self, direction: MotionDirection) {
        match direction {
            MotionDirection::DispenseTowardEmpty => self.dir.set_low(),
            MotionDirection::RetractTowardLoad => self.dir.set_high(),
        }
    }
}

// Motor task command protocol.

#[derive(Clone, Copy, Debug)]
enum MotorCommand {
    Stop {
        command_id: u32,
    },
    SetPosition {
        command_id: u32,
        position_steps: i32,
    },
    Run {
        command_id: u32,
        kind: MotorRunKind,
        direction: MotionDirection,
        period_us: u64,
        max_steps: Option<u32>,
    },
    Tone {
        command_id: u32,
        period_us: u64,
        duration_ms: u64,
    },
}

struct ActiveRun {
    command_id: u32,
    kind: MotorRunKind,
    direction: MotionDirection,
    period: Duration,
    period_us: u64,
    target_period_us: u64,
    ramp_start_period_us: u64,
    ramp_steps: u32,
    max_steps: Option<u32>,
    command_steps: u32,
    tone_until: Option<Instant>,
    next_step: Instant,
}

// Global command/status channels.

static MOTOR_COMMANDS: Channel<CriticalSectionRawMutex, MotorCommand, 8> = Channel::new();
static MOTOR_STATUS: Signal<CriticalSectionRawMutex, MotorStatus> = Signal::new();
static NEXT_COMMAND_ID: AtomicU32 = AtomicU32::new(1);

// RMT chunk sizing.

const RMT_FAST_CHUNK_STEPS: usize = 128;
const RMT_DELIVERY_CHUNK_STEPS: usize = 1;
const RMT_POSITIONING_CHUNK_STEPS: usize = 16;
const RMT_TONE_CHUNK_STEPS: usize = 1;
const RMT_MAX_DIRECT_PERIOD_US: u64 = PulseCode::MAX_LEN as u64 + STEP_HIGH_US as u64;

// Public async client used by UI and startup code.

#[derive(Clone, Copy)]
pub struct MotorClient;

impl MotorClient {
    pub const fn new() -> Self {
        Self
    }

    pub fn try_status(self) -> Option<MotorStatus> {
        MOTOR_STATUS.try_take()
    }

    pub async fn stop(self, command_id: u32) {
        MOTOR_COMMANDS.send(MotorCommand::Stop { command_id }).await;
    }

    pub async fn set_position(self, command_id: u32, position_steps: i32) {
        MOTOR_COMMANDS
            .send(MotorCommand::SetPosition {
                command_id,
                position_steps,
            })
            .await;
    }

    pub async fn run(
        self,
        command_id: u32,
        kind: MotorRunKind,
        direction: MotionDirection,
        period_us: u64,
        max_steps: Option<u32>,
    ) {
        MOTOR_COMMANDS
            .send(MotorCommand::Run {
                command_id,
                kind,
                direction,
                period_us,
                max_steps,
            })
            .await;
    }

    pub async fn tone(self, command_id: u32, period_us: u64, duration_ms: u64) {
        MOTOR_COMMANDS
            .send(MotorCommand::Tone {
                command_id,
                period_us,
                duration_ms,
            })
            .await;
    }

    pub async fn wait_complete(self, command_id: u32) -> MotorStatus {
        loop {
            let status = MOTOR_STATUS.wait().await;
            if status.completed_command_id == command_id {
                return status;
            }
        }
    }

    pub async fn move_steps(
        self,
        command_id: u32,
        direction: MotionDirection,
        steps: u32,
        period_us: u64,
    ) -> MotorStatus {
        self.run(
            command_id,
            MotorRunKind::Positioning,
            direction,
            period_us,
            Some(steps),
        )
        .await;
        self.wait_complete(command_id).await
    }

    pub fn next_command_id(self) -> u32 {
        NEXT_COMMAND_ID.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn stop_now(self) -> MotorStatus {
        let command_id = self.next_command_id();
        self.stop(command_id).await;
        self.wait_complete(command_id).await
    }

    pub fn disable(self) {
        let command_id = self.next_command_id();
        let _ = MOTOR_COMMANDS.try_send(MotorCommand::Stop { command_id });
    }

    pub async fn set_position_now(self, position_steps: i32) -> MotorStatus {
        let command_id = self.next_command_id();
        self.set_position(command_id, position_steps).await;
        self.wait_complete(command_id).await
    }

    pub async fn run_auto(
        self,
        kind: MotorRunKind,
        direction: MotionDirection,
        period_us: u64,
        max_steps: Option<u32>,
    ) -> u32 {
        let command_id = self.next_command_id();
        self.run(command_id, kind, direction, period_us, max_steps)
            .await;
        command_id
    }

    pub async fn move_steps_auto(
        self,
        direction: MotionDirection,
        steps: u32,
        period_us: u64,
    ) -> MotorStatus {
        let command_id = self.next_command_id();
        self.move_steps(command_id, direction, steps, period_us)
            .await
    }

    pub async fn tone_auto(self, period_us: u64, duration_ms: u64) -> MotorStatus {
        let command_id = self.next_command_id();
        self.tone(command_id, period_us, duration_ms).await;
        self.wait_complete(command_id).await
    }
}

/// Owns the motor pins, executes queued commands, and publishes step/position status.
#[embassy_executor::task]
pub async fn motor_task(mut motor: MotorPins) -> ! {
    let mut status = MotorStatus::idle();
    let mut active: Option<ActiveRun> = None;

    motor.disable();
    MOTOR_STATUS.signal(status);

    loop {
        match active.as_mut() {
            Some(_) => {
                while let Ok(command) = MOTOR_COMMANDS.try_receive() {
                    handle_command(command, &mut motor, &mut status, &mut active);
                    if active.is_none() {
                        break;
                    }
                }

                if let Some(run) = active.as_mut() {
                    let now = Instant::now();
                    if run.next_step > now {
                        let wait = run.next_step - now;
                        if wait > Duration::from_millis(1) {
                            Timer::after_millis(1).await;
                        } else {
                            Timer::at(run.next_step).await;
                        }
                        continue;
                    }

                    if run.tone_until.map(|until| now >= until).unwrap_or(false) {
                        complete_active(&mut motor, &mut status, &mut active);
                        continue;
                    }

                    let stepped = transmit_ready_steps(&mut motor, run, &mut status).await;
                    if !stepped {
                        complete_active(&mut motor, &mut status, &mut active);
                        continue;
                    }

                    if run
                        .max_steps
                        .map(|steps| run.command_steps >= steps)
                        .unwrap_or(false)
                    {
                        complete_active(&mut motor, &mut status, &mut active);
                    }
                }
            }
            None => {
                let command = MOTOR_COMMANDS.receive().await;
                handle_command(command, &mut motor, &mut status, &mut active);
            }
        }
    }
}

/// Sends the next ready chunk of STEP pulses through RMT and records completed steps.
async fn transmit_ready_steps(
    motor: &mut MotorPins,
    run: &mut ActiveRun,
    status: &mut MotorStatus,
) -> bool {
    motor.enable();
    motor.set_motion_direction(run.direction);

    let step_count = rmt_chunk_step_count(run);
    let mut codes = [PulseCode::end_marker(); RMT_FAST_CHUNK_STEPS + 1];
    let mut simulated_command_steps = run.command_steps;
    for code in codes.iter_mut().take(step_count) {
        let period_us = period_us_for_completed_steps(run, simulated_command_steps);
        *code = step_pulse_code(period_us);
        simulated_command_steps = simulated_command_steps.saturating_add(1);
    }
    codes[step_count] = PulseCode::end_marker();

    match motor.step.transmit(&codes[..=step_count]).await {
        Ok(()) => {
            for _ in 0..step_count {
                record_step(run, status);
                update_run_period(run);
                run.next_step += run.period;
            }
            MOTOR_STATUS.signal(*status);
            true
        }
        Err(_) => {
            log::error!("RMT STEP pulse transmission failed");
            false
        }
    }
}

/// Chooses the largest safe RMT chunk for the active run and remaining step count.
fn rmt_chunk_step_count(run: &ActiveRun) -> usize {
    let max_chunk = match run.kind {
        // Normal perfusion must react immediately to pause; a large queued RMT chunk
        // would continue dispensing before the Stop command can be processed.
        MotorRunKind::Delivery => RMT_DELIVERY_CHUNK_STEPS,
        MotorRunKind::DirectBolus => RMT_FAST_CHUNK_STEPS,
        MotorRunKind::Positioning => RMT_POSITIONING_CHUNK_STEPS,
        MotorRunKind::Tone => RMT_TONE_CHUNK_STEPS,
        MotorRunKind::Idle => 1,
    };
    let remaining_steps = run
        .max_steps
        .map(|max_steps| max_steps.saturating_sub(run.command_steps) as usize)
        .unwrap_or(max_chunk);
    let period_limited_chunk = if run.period_us <= RMT_MAX_DIRECT_PERIOD_US {
        max_chunk
    } else {
        1
    };
    remaining_steps.min(period_limited_chunk).max(1)
}

/// Builds one STEP pulse, keeping the high time fixed and the low time rate-dependent.
fn step_pulse_code(period_us: u64) -> PulseCode {
    let high_ticks = STEP_HIGH_US as u16;
    let low_us = period_us.saturating_sub(STEP_HIGH_US as u64).max(1);
    let low_ticks = low_us.min(PulseCode::MAX_LEN as u64) as u16;
    PulseCode::new(Level::High, high_ticks, Level::Low, low_ticks)
}

/// Computes the ramped period for a future step without mutating the active run.
fn period_us_for_completed_steps(run: &ActiveRun, completed_steps: u32) -> u64 {
    if run.ramp_steps == 0 || run.ramp_start_period_us <= run.target_period_us {
        return run.target_period_us;
    }

    let ramp_step = completed_steps.min(run.ramp_steps);
    let delta = run.ramp_start_period_us - run.target_period_us;
    run.ramp_start_period_us - delta * ramp_step as u64 / run.ramp_steps as u64
}

/// Updates command counters and signed carriage position after one physical step.
fn record_step(run: &mut ActiveRun, status: &mut MotorStatus) {
    run.command_steps = run.command_steps.saturating_add(1);
    status.command_steps = run.command_steps;
    status.total_steps = status.total_steps.saturating_add(1);
    match run.direction {
        MotionDirection::DispenseTowardEmpty => {
            status.position_steps = status.position_steps.saturating_add(1);
        }
        MotionDirection::RetractTowardLoad => {
            status.position_steps = status.position_steps.saturating_sub(1);
        }
    }
}

/// Advances the active run period along the delivery acceleration ramp.
fn update_run_period(run: &mut ActiveRun) {
    if run.ramp_steps == 0 || run.ramp_start_period_us <= run.target_period_us {
        return;
    }

    let ramp_step = run.command_steps.min(run.ramp_steps);
    let delta = run.ramp_start_period_us - run.target_period_us;
    let period_us = run.ramp_start_period_us - delta * ramp_step as u64 / run.ramp_steps as u64;
    run.period_us = period_us;
    run.period = Duration::from_micros(period_us);
}

/// Applies a queued motor command to pins, status, and the active run state.
fn handle_command(
    command: MotorCommand,
    motor: &mut MotorPins,
    status: &mut MotorStatus,
    active: &mut Option<ActiveRun>,
) {
    match command {
        MotorCommand::Stop { command_id } => {
            motor.disable();
            *active = None;
            status.command_id = command_id;
            status.completed_command_id = command_id;
            status.running = false;
            status.kind = MotorRunKind::Idle;
            status.command_steps = 0;
            MOTOR_STATUS.signal(*status);
        }
        MotorCommand::SetPosition {
            command_id,
            position_steps,
        } => {
            status.command_id = command_id;
            status.completed_command_id = command_id;
            status.position_steps = position_steps;
            MOTOR_STATUS.signal(*status);
        }
        MotorCommand::Run {
            command_id,
            kind,
            direction,
            period_us,
            max_steps,
        } => {
            let target_period_us = period_us.max(STEP_HIGH_US + 1);
            let ramp_start_period_us = if kind == MotorRunKind::Delivery
                && max_steps.is_some()
                && target_period_us < DISPENSE_START_STEP_PERIOD_US
            {
                DISPENSE_START_STEP_PERIOD_US
            } else {
                target_period_us
            };
            let ramp_steps = if ramp_start_period_us > target_period_us {
                DISPENSE_RAMP_STEPS
            } else {
                0
            };
            motor.enable();
            motor.set_motion_direction(direction);
            status.command_id = command_id;
            status.completed_command_id = 0;
            status.running = true;
            status.kind = kind;
            status.command_steps = 0;
            *active = Some(ActiveRun {
                command_id,
                kind,
                direction,
                period: Duration::from_micros(ramp_start_period_us),
                period_us: ramp_start_period_us,
                target_period_us,
                ramp_start_period_us,
                ramp_steps,
                max_steps,
                command_steps: 0,
                tone_until: None,
                next_step: Instant::now(),
            });
            MOTOR_STATUS.signal(*status);
        }
        MotorCommand::Tone {
            command_id,
            period_us,
            duration_ms,
        } => {
            let period_us = period_us.max(STEP_HIGH_US + 1);
            motor.enable();
            motor.set_motion_direction(MotionDirection::DispenseTowardEmpty);
            status.command_id = command_id;
            status.completed_command_id = 0;
            status.running = true;
            status.kind = MotorRunKind::Tone;
            status.command_steps = 0;
            let now = Instant::now();
            *active = Some(ActiveRun {
                command_id,
                kind: MotorRunKind::Tone,
                direction: MotionDirection::DispenseTowardEmpty,
                period: Duration::from_micros(period_us),
                period_us,
                target_period_us: period_us,
                ramp_start_period_us: period_us,
                ramp_steps: 0,
                max_steps: None,
                command_steps: 0,
                tone_until: Some(now + Duration::from_millis(duration_ms)),
                next_step: now,
            });
            MOTOR_STATUS.signal(*status);
        }
    }
}

/// Finishes the active command, disables the driver, and signals command completion.
fn complete_active(
    motor: &mut MotorPins,
    status: &mut MotorStatus,
    active: &mut Option<ActiveRun>,
) {
    if let Some(run) = active.take() {
        motor.disable();
        status.command_id = run.command_id;
        status.completed_command_id = run.command_id;
        status.running = false;
        status.kind = MotorRunKind::Idle;
        status.command_steps = run.command_steps;
        MOTOR_STATUS.signal(*status);
    }
}

pub fn tone_microsteps_from_status(status: MotorStatus) -> u32 {
    status.command_steps.saturating_mul(TONE_FULL_STEP_SCALE)
}
