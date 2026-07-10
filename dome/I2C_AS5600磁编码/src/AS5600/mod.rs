//! AS5600 12-bit magnetic rotary encoder driver.
//!
//! Provides both blocking ([`blocking::BlockingAs5600`]) and async ([`r#async::AsyncAs5600`])
//! drivers for the AMS AS5600 magnetic angle sensor.
//!
//! # Register Map
//!
//! | Address | Name       | Size | Access | Description              |
//! |---------|------------|------|--------|--------------------------|
//! | 0x00    | ZMCO       | 1    | R      | Zero position burn count |
//! | 0x01    | ZPOS       | 2    | R/W    | Zero position (12-bit)   |
//! | 0x03    | MPOS       | 2    | R/W    | Maximum position (12-bit)|
//! | 0x05    | MANG       | 2    | R/W    | Maximum angle (12-bit)   |
//! | 0x07    | CONF       | 2    | R/W    | Configuration (14-bit)   |
//! | 0x0B    | STATUS     | 1    | R      | Status register          |
//! | 0x0C    | RAW_ANGLE  | 2    | R      | Raw 12-bit angle         |
//! | 0x0E    | ANGLE      | 2    | R      | Output angle (12-bit)    |
//! | 0x1A    | AGC        | 1    | R      | Automatic gain control   |
//! | 0x1B    | MAGNITUDE  | 2    | R      | CORDIC magnitude         |
//! | 0xFF    | BURN       | 1    | W      | Burn to OTP              |

pub mod blocking;
#[path = "async.rs"]
pub mod r#async;

use core::f32::consts::PI;

// ---------------------------------------------------------------------------
//  I2C Address
// ---------------------------------------------------------------------------

/// Default I2C address of AS5600 (7-bit).
pub const ADDRESS: u8 = 0x36;

// ---------------------------------------------------------------------------
//  Register Addresses
// ---------------------------------------------------------------------------

pub(crate) const REG_ZMCO: u8 = 0x00;
pub(crate) const REG_ZPOS: u8 = 0x01;
pub(crate) const REG_MPOS: u8 = 0x03;
pub(crate) const REG_MANG: u8 = 0x05;
pub(crate) const REG_CONF: u8 = 0x07;
pub(crate) const REG_STATUS: u8 = 0x0B;
pub(crate) const REG_RAW_ANGLE: u8 = 0x0C;
pub(crate) const REG_ANGLE: u8 = 0x0E;
pub(crate) const REG_AGC: u8 = 0x1A;
pub(crate) const REG_MAGNITUDE: u8 = 0x1B;
pub(crate) const REG_BURN: u8 = 0xFF;

// ---------------------------------------------------------------------------
//  Burn Commands
// ---------------------------------------------------------------------------

pub(crate) const BURN_ANGLE: u8 = 0x00;
pub(crate) const BURN_SETTING: u8 = 0x80;

// ---------------------------------------------------------------------------
//  Bit Masks & Shifts
// ---------------------------------------------------------------------------

/// Mask for 12-bit angle values.
pub const ANGLE_MASK: u16 = 0x0FFF;

// Status register bits
pub(crate) const STATUS_MH: u8 = 0x08; // magnet too strong
pub(crate) const STATUS_ML: u8 = 0x10; // magnet too weak
pub(crate) const STATUS_MD: u8 = 0x20; // magnet detected

// CONF register bit masks
pub(crate) const CONF_PM_MASK: u16 = 0x0003;
pub(crate) const CONF_HYST_MASK: u16 = 0x000C;
pub(crate) const CONF_OUTS_MASK: u16 = 0x0030;
pub(crate) const CONF_PWMF_MASK: u16 = 0x00C0;
pub(crate) const CONF_SF_MASK: u16 = 0x0300;
pub(crate) const CONF_FTH_MASK: u16 = 0x1C00;
pub(crate) const CONF_WD_MASK: u16 = 0x2000;

pub(crate) const CONF_PM_SHIFT: u8 = 0;
pub(crate) const CONF_HYST_SHIFT: u8 = 2;
pub(crate) const CONF_OUTS_SHIFT: u8 = 4;
pub(crate) const CONF_PWMF_SHIFT: u8 = 6;
pub(crate) const CONF_SF_SHIFT: u8 = 8;
pub(crate) const CONF_FTH_SHIFT: u8 = 10;
pub(crate) const CONF_WD_SHIFT: u8 = 13;

// ---------------------------------------------------------------------------
//  Conversion Constants
// ---------------------------------------------------------------------------

/// Convert raw 12-bit value to degrees (0.0 .. 360.0).
pub const RAW_TO_DEGREES: f32 = 360.0 / 4096.0;

/// Convert raw 12-bit value to radians (0.0 .. 2π).
pub const RAW_TO_RADIANS: f32 = PI * 2.0 / 4096.0;

// ---------------------------------------------------------------------------
//  Error Type
// ---------------------------------------------------------------------------

/// All possible errors from the AS5600 driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    /// I2C bus error.
    I2c(E),
    /// Failed to parse status register value.
    InvalidStatus,
}

// ---------------------------------------------------------------------------
//  Magnet Status
// ---------------------------------------------------------------------------

/// High-level magnet detection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagnetStatus {
    /// Magnet detected and within acceptable range.
    Detected,
    /// Magnet detected but too weak.
    TooWeak,
    /// Magnet detected but too strong.
    TooStrong,
    /// No magnet detected.
    NotDetected,
}

// ---------------------------------------------------------------------------
//  Status Register
// ---------------------------------------------------------------------------

/// Parsed status register value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// Magnet detected (MD bit).
    pub magnet_detected: bool,
    /// Magnet too weak (ML bit).
    pub magnet_too_weak: bool,
    /// Magnet too strong (MH bit).
    pub magnet_too_strong: bool,
}

impl Status {
    /// Parse a status register byte.
    #[inline]
    pub fn from_byte(byte: u8) -> Self {
        Self {
            magnet_detected: (byte & STATUS_MD) != 0,
            magnet_too_weak: (byte & STATUS_ML) != 0,
            magnet_too_strong: (byte & STATUS_MH) != 0,
        }
    }

    /// Convenience: return a [`MagnetStatus`] enum based on the flags.
    #[inline]
    pub fn magnet_status(&self) -> MagnetStatus {
        if !self.magnet_detected {
            MagnetStatus::NotDetected
        } else if self.magnet_too_strong {
            MagnetStatus::TooStrong
        } else if self.magnet_too_weak {
            MagnetStatus::TooWeak
        } else {
            MagnetStatus::Detected
        }
    }
}

// ---------------------------------------------------------------------------
//  Configuration Enums
// ---------------------------------------------------------------------------

/// Power mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    /// Normal power mode.
    Normal,
    /// Low power mode 1.
    Low1,
    /// Low power mode 2.
    Low2,
    /// Low power mode 3 (lowest power).
    Low3,
}

impl PowerMode {
    fn from_bits(bits: u16) -> Self {
        match bits & 0x03 {
            0 => Self::Normal,
            1 => Self::Low1,
            2 => Self::Low2,
            3 => Self::Low3,
            _ => unreachable!(),
        }
    }

    fn to_bits(self) -> u16 {
        match self {
            Self::Normal => 0,
            Self::Low1 => 1,
            Self::Low2 => 2,
            Self::Low3 => 3,
        }
    }
}

/// Hysteresis setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hysteresis {
    /// Hysteresis off.
    Off,
    /// 1 LSB hysteresis.
    Lsb1,
    /// 2 LSB hysteresis.
    Lsb2,
    /// 3 LSB hysteresis.
    Lsb3,
}

impl Hysteresis {
    fn from_bits(bits: u16) -> Self {
        match (bits >> 2) & 0x03 {
            0 => Self::Off,
            1 => Self::Lsb1,
            2 => Self::Lsb2,
            3 => Self::Lsb3,
            _ => unreachable!(),
        }
    }

    fn to_bits(self) -> u16 {
        (match self {
            Self::Off => 0,
            Self::Lsb1 => 1,
            Self::Lsb2 => 2,
            Self::Lsb3 => 3,
        }) << 2
    }
}

/// Output stage mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStage {
    /// Analog output, full range (0% – 100%).
    AnalogFull,
    /// Analog output, reduced range (10% – 90%).
    AnalogReduced,
    /// PWM output.
    Pwm,
}

impl OutputStage {
    fn from_bits(bits: u16) -> Self {
        match (bits >> 4) & 0x03 {
            0 => Self::AnalogFull,
            1 => Self::AnalogReduced,
            2 | _ => Self::Pwm,
        }
    }

    fn to_bits(self) -> u16 {
        (match self {
            Self::AnalogFull => 0,
            Self::AnalogReduced => 1,
            Self::Pwm => 2,
        }) << 4
    }
}

/// PWM frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PwmFreq {
    /// 115 Hz.
    Freq115Hz,
    /// 230 Hz.
    Freq230Hz,
    /// 460 Hz.
    Freq460Hz,
    /// 920 Hz.
    Freq920Hz,
}

impl PwmFreq {
    fn from_bits(bits: u16) -> Self {
        match (bits >> 6) & 0x03 {
            0 => Self::Freq115Hz,
            1 => Self::Freq230Hz,
            2 => Self::Freq460Hz,
            3 => Self::Freq920Hz,
            _ => unreachable!(),
        }
    }

    fn to_bits(self) -> u16 {
        (match self {
            Self::Freq115Hz => 0,
            Self::Freq230Hz => 1,
            Self::Freq460Hz => 2,
            Self::Freq920Hz => 3,
        }) << 6
    }
}

/// Slow filter mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlowFilter {
    /// 16× averaging.
    X16,
    /// 8× averaging.
    X8,
    /// 4× averaging.
    X4,
    /// 2× averaging.
    X2,
}

impl SlowFilter {
    fn from_bits(bits: u16) -> Self {
        match (bits >> 8) & 0x03 {
            0 => Self::X16,
            1 => Self::X8,
            2 => Self::X4,
            3 => Self::X2,
            _ => unreachable!(),
        }
    }

    fn to_bits(self) -> u16 {
        (match self {
            Self::X16 => 0,
            Self::X8 => 1,
            Self::X4 => 2,
            Self::X2 => 3,
        }) << 8
    }
}

/// Fast filter threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastFilterThreshold {
    /// No fast filter.
    None,
    /// 6 LSB threshold.
    Lsb6,
    /// 7 LSB threshold.
    Lsb7,
    /// 9 LSB threshold.
    Lsb9,
    /// 18 LSB threshold.
    Lsb18,
    /// 21 LSB threshold.
    Lsb21,
    /// 24 LSB threshold.
    Lsb24,
    /// 10 LSB threshold.
    Lsb10,
}

impl FastFilterThreshold {
    fn from_bits(bits: u16) -> Self {
        match (bits >> 10) & 0x07 {
            0 => Self::None,
            1 => Self::Lsb6,
            2 => Self::Lsb7,
            3 => Self::Lsb9,
            4 => Self::Lsb18,
            5 => Self::Lsb21,
            6 => Self::Lsb24,
            7 => Self::Lsb10,
            _ => unreachable!(),
        }
    }

    fn to_bits(self) -> u16 {
        (match self {
            Self::None => 0,
            Self::Lsb6 => 1,
            Self::Lsb7 => 2,
            Self::Lsb9 => 3,
            Self::Lsb18 => 4,
            Self::Lsb21 => 5,
            Self::Lsb24 => 6,
            Self::Lsb10 => 7,
        }) << 10
    }
}

/// Watchdog state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watchdog {
    /// Watchdog disabled.
    Off,
    /// Watchdog enabled.
    On,
}

impl Watchdog {
    fn from_bits(bits: u16) -> Self {
        if (bits >> 13) & 0x01 == 0 {
            Self::Off
        } else {
            Self::On
        }
    }

    fn to_bits(self) -> u16 {
        (match self {
            Self::Off => 0,
            Self::On => 1,
        }) << 13
    }
}

// ---------------------------------------------------------------------------
//  Configuration
// ---------------------------------------------------------------------------

/// Complete AS5600 configuration register value.
///
/// Use [`Configuration::default()`] to get the factory-default configuration,
/// then modify fields as needed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Configuration {
    /// Power mode.
    pub power_mode: PowerMode,
    /// Hysteresis.
    pub hysteresis: Hysteresis,
    /// Output stage.
    pub output_stage: OutputStage,
    /// PWM frequency.
    pub pwm_frequency: PwmFreq,
    /// Slow filter mode.
    pub slow_filter: SlowFilter,
    /// Fast filter threshold.
    pub fast_filter_threshold: FastFilterThreshold,
    /// Watchdog.
    pub watchdog: Watchdog,
}

impl Configuration {
    /// Factory-default configuration (all fields = 0).
    pub const fn default() -> Self {
        Self {
            power_mode: PowerMode::Normal,
            hysteresis: Hysteresis::Off,
            output_stage: OutputStage::AnalogFull,
            pwm_frequency: PwmFreq::Freq115Hz,
            slow_filter: SlowFilter::X16,
            fast_filter_threshold: FastFilterThreshold::None,
            watchdog: Watchdog::Off,
        }
    }

    /// Parse a 16-bit CONF register value.
    #[inline]
    pub fn from_u16(raw: u16) -> Self {
        Self {
            power_mode: PowerMode::from_bits(raw),
            hysteresis: Hysteresis::from_bits(raw),
            output_stage: OutputStage::from_bits(raw),
            pwm_frequency: PwmFreq::from_bits(raw),
            slow_filter: SlowFilter::from_bits(raw),
            fast_filter_threshold: FastFilterThreshold::from_bits(raw),
            watchdog: Watchdog::from_bits(raw),
        }
    }

    /// Serialize to a 16-bit CONF register value.
    #[inline]
    pub fn to_u16(&self) -> u16 {
        self.power_mode.to_bits()
            | self.hysteresis.to_bits()
            | self.output_stage.to_bits()
            | self.pwm_frequency.to_bits()
            | self.slow_filter.to_bits()
            | self.fast_filter_threshold.to_bits()
            | self.watchdog.to_bits()
    }
}

// ---------------------------------------------------------------------------
//  Angle Conversion Functions
// ---------------------------------------------------------------------------

/// Convert a raw 12-bit angle value to degrees (0.0 .. 360.0).
#[inline]
pub fn raw_to_degrees(raw: u16) -> f32 {
    (raw & ANGLE_MASK) as f32 * RAW_TO_DEGREES
}

/// Convert a raw 12-bit angle value to radians (0.0 .. 2π).
#[inline]
pub fn raw_to_radians(raw: u16) -> f32 {
    (raw & ANGLE_MASK) as f32 * RAW_TO_RADIANS
}
