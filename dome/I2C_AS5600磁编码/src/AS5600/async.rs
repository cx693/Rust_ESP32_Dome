//! Async AS5600 driver using [`embedded_hal_async::i2c::I2c`].

use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::i2c::I2c;

use super::{
    raw_to_degrees, raw_to_radians, Configuration, Error, MagnetStatus, Status, ADDRESS, ANGLE_MASK,
    BURN_ANGLE, BURN_SETTING, REG_AGC, REG_ANGLE, REG_BURN, REG_CONF, REG_MAGNITUDE, REG_MANG,
    REG_MPOS, REG_RAW_ANGLE, REG_STATUS, REG_ZMCO, REG_ZPOS,
};

/// Async AS5600 driver.
///
/// Generic over any I2C bus implementing [`embedded_hal_async::i2c::I2c`].
///
/// # Example
///
/// ```ignore
/// use as5600::r#async::AsyncAs5600;
///
/// let mut encoder = AsyncAs5600::new(i2c);
/// let angle = encoder.angle().await.unwrap();
/// ```
pub struct AsyncAs5600<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> AsyncAs5600<I2C> {
    /// Create a new driver instance with the default I2C address (0x36).
    #[inline]
    pub fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: ADDRESS,
        }
    }

    /// Create a new driver instance with a custom I2C address.
    ///
    /// This is useful for the AS5600L variant or when using an address translator.
    #[inline]
    pub fn with_address(address: u8, i2c: I2C) -> Self {
        Self { i2c, address }
    }

    /// Release the I2C bus, consuming the driver.
    #[inline]
    pub fn release(self) -> I2C {
        self.i2c
    }

    /// Return the current I2C address.
    #[inline]
    pub fn address(&self) -> u8 {
        self.address
    }
}

impl<I2C: I2c> AsyncAs5600<I2C> {
    // -----------------------------------------------------------------------
    //  Read-only Output Registers
    // -----------------------------------------------------------------------

    /// Read the raw 12-bit angle (register 0x0C–0x0D).
    ///
    /// This value is **not** affected by ZPOS or MPOS.
    #[inline]
    pub async fn raw_angle(&mut self) -> Result<u16, Error<I2C::Error>> {
        self.read_u16(REG_RAW_ANGLE).await
    }

    /// Read the output 12-bit angle (register 0x0E–0x0F).
    ///
    /// This value **is** affected by ZPOS and MPOS settings.
    #[inline]
    pub async fn angle(&mut self) -> Result<u16, Error<I2C::Error>> {
        self.read_u16(REG_ANGLE).await
    }

    /// Read the status register (0x0B).
    #[inline]
    pub async fn status(&mut self) -> Result<Status, Error<I2C::Error>> {
        let mut buf = [0u8; 1];
        self.i2c
            .write_read(self.address, &[REG_STATUS], &mut buf)
            .await
            .map_err(Error::I2c)?;
        Ok(Status::from_byte(buf[0]))
    }

    /// Convenience: read the magnet detection status.
    #[inline]
    pub async fn magnet_status(&mut self) -> Result<MagnetStatus, Error<I2C::Error>> {
        let s: Status = self.status().await?;
        Ok(s.magnet_status())
    }

    /// Read the Automatic Gain Control value (0x1A).
    ///
    /// Range: 0–255 in 5 V mode, 0–128 in 3.3 V mode.
    #[inline]
    pub async fn agc(&mut self) -> Result<u8, Error<I2C::Error>> {
        let mut buf = [0u8; 1];
        self.i2c
            .write_read(self.address, &[REG_AGC], &mut buf)
            .await
            .map_err(Error::I2c)?;
        Ok(buf[0])
    }

    /// Read the CORDIC magnitude (0x1B–0x1C).
    ///
    /// Indicates relative magnetic field strength.
    #[inline]
    pub async fn magnitude(&mut self) -> Result<u16, Error<I2C::Error>> {
        self.read_u16(REG_MAGNITUDE).await
    }

    /// Read the zero-motion OTP burn count (0x00).
    ///
    /// Returns how many times ZPOS and MPOS have been burned to OTP (max 3).
    #[inline]
    pub async fn zmco(&mut self) -> Result<u8, Error<I2C::Error>> {
        let mut buf = [0u8; 1];
        self.i2c
            .write_read(self.address, &[REG_ZMCO], &mut buf)
            .await
            .map_err(Error::I2c)?;
        Ok(buf[0])
    }

    // -----------------------------------------------------------------------
    //  Configuration (Read / Write)
    // -----------------------------------------------------------------------

    /// Read the configuration register (0x07–0x08).
    #[inline]
    pub async fn config(&mut self) -> Result<Configuration, Error<I2C::Error>> {
        let raw = self.read_u16(REG_CONF).await?;
        Ok(Configuration::from_u16(raw))
    }

    /// Write the configuration register (0x07–0x08).
    #[inline]
    pub async fn set_config(&mut self, config: &Configuration) -> Result<(), Error<I2C::Error>> {
        self.write_u16(REG_CONF, config.to_u16()).await
    }

    // -----------------------------------------------------------------------
    //  Programmable Position Registers (Read / Write)
    // -----------------------------------------------------------------------

    /// Read the zero position (0x01–0x02), 12-bit.
    #[inline]
    pub async fn zero_position(&mut self) -> Result<u16, Error<I2C::Error>> {
        self.read_u16(REG_ZPOS).await
    }

    /// Set the zero position (0x01–0x02), 12-bit.
    ///
    /// The value is masked to 12 bits.
    #[inline]
    pub async fn set_zero_position(&mut self, pos: u16) -> Result<(), Error<I2C::Error>> {
        self.write_u16(REG_ZPOS, pos & ANGLE_MASK).await
    }

    /// Read the maximum position (0x03–0x04), 12-bit.
    #[inline]
    pub async fn max_position(&mut self) -> Result<u16, Error<I2C::Error>> {
        self.read_u16(REG_MPOS).await
    }

    /// Set the maximum position (0x03–0x04), 12-bit.
    ///
    /// The value is masked to 12 bits.
    #[inline]
    pub async fn set_max_position(&mut self, pos: u16) -> Result<(), Error<I2C::Error>> {
        self.write_u16(REG_MPOS, pos & ANGLE_MASK).await
    }

    /// Read the maximum angle (0x05–0x06), 12-bit.
    #[inline]
    pub async fn max_angle(&mut self) -> Result<u16, Error<I2C::Error>> {
        self.read_u16(REG_MANG).await
    }

    /// Set the maximum angle (0x05–0x06), 12-bit.
    ///
    /// The value is masked to 12 bits.
    #[inline]
    pub async fn set_max_angle(&mut self, angle: u16) -> Result<(), Error<I2C::Error>> {
        self.write_u16(REG_MANG, angle & ANGLE_MASK).await
    }

    // -----------------------------------------------------------------------
    //  Persistence (Burn to OTP)
    // -----------------------------------------------------------------------

    /// Burn ZPOS and MPOS to OTP (up to 3 times).
    ///
    /// A delay of at least 5 ms is required after the burn command.
    pub async fn burn_angle<D: DelayNs>(&mut self, delay: &mut D) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[REG_BURN, BURN_ANGLE])
            .await
            .map_err(Error::I2c)?;
        delay.delay_ms(5).await;
        Ok(())
    }

    /// Burn MANG and CONFIG to OTP (permanent, one-time only).
    ///
    /// A delay of at least 5 ms is required after the burn command.
    pub async fn burn_setting<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[REG_BURN, BURN_SETTING])
            .await
            .map_err(Error::I2c)?;
        delay.delay_ms(5).await;
        Ok(())
    }

    // -----------------------------------------------------------------------
    //  Angle Conversions (convenience)
    // -----------------------------------------------------------------------

    /// Read the output angle and convert to degrees.
    #[inline]
    pub async fn angle_degrees(&mut self) -> Result<f32, Error<I2C::Error>> {
        let raw: u16 = self.angle().await?;
        Ok(raw_to_degrees(raw))
    }

    /// Read the output angle and convert to radians.
    #[inline]
    pub async fn angle_radians(&mut self) -> Result<f32, Error<I2C::Error>> {
        let raw: u16 = self.angle().await?;
        Ok(raw_to_radians(raw))
    }

    /// Read the raw angle and convert to degrees.
    #[inline]
    pub async fn raw_angle_degrees(&mut self) -> Result<f32, Error<I2C::Error>> {
        let raw: u16 = self.raw_angle().await?;
        Ok(raw_to_degrees(raw))
    }

    /// Read the raw angle and convert to radians.
    #[inline]
    pub async fn raw_angle_radians(&mut self) -> Result<f32, Error<I2C::Error>> {
        let raw: u16 = self.raw_angle().await?;
        Ok(raw_to_radians(raw))
    }

    // -----------------------------------------------------------------------
    //  Internal I2C Helpers
    // -----------------------------------------------------------------------

    /// Read a 16-bit big-endian register, masked to 12 bits.
    #[inline]
    async fn read_u16(&mut self, reg: u8) -> Result<u16, Error<I2C::Error>> {
        let mut buf = [0u8; 2];
        self.i2c
            .write_read(self.address, &[reg], &mut buf)
            .await
            .map_err(Error::I2c)?;
        Ok((u16::from_be_bytes(buf)) & ANGLE_MASK)
    }

    /// Write a 16-bit big-endian value to a register.
    #[inline]
    async fn write_u16(&mut self, reg: u8, value: u16) -> Result<(), Error<I2C::Error>> {
        let buf = [reg, (value >> 8) as u8, (value & 0xFF) as u8];
        self.i2c.write(self.address, &buf).await.map_err(Error::I2c)
    }
}
