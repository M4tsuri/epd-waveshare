//! A simple Driver for the Waveshare 1.02" E-Ink Display via SPI

use embedded_hal::{
    blocking::{delay::*, spi::Write},
    digital::v2::*,
};

use crate::interface::DisplayInterface;
use crate::traits::{
    InternalWiAdditions, RefreshLut, WaveshareDisplay,
};

/// Width of epd1in02 in pixels
pub const WIDTH: u32 = 80;
/// Height of epd1in02 in pixels
pub const HEIGHT: u32 = 128;
/// Default Background Color (white)
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const IS_BUSY_LOW: bool = true;
const NUM_DISPLAY_BITS: u32 = WIDTH * HEIGHT / 8;

use crate::color::Color;

pub(crate) mod command;
pub(crate) mod constants;
use self::{command::Command, constants::{LUT_W1, LUT_B1}};

#[cfg(feature = "graphics")]
mod graphics;

#[cfg(feature = "graphics")]
pub use self::graphics::Display1in02;

/// Epd1in02 driver
pub struct Epd1in02<SPI, CS, BUSY, DC, RST, DELAY> {
    interface: DisplayInterface<SPI, CS, BUSY, DC, RST, DELAY>,
    color: Color,
}

impl<SPI, CS, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, CS, BUSY, DC, RST, DELAY>
    for Epd1in02<SPI, CS, BUSY, DC, RST, DELAY>
where
    SPI: Write<u8>,
    CS: OutputPin,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayMs<u8>,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Based on Reference Program Code from:
        // https://www.waveshare.com/w/upload/a/ac/1.54inch_e-Paper_Module_C_Specification.pdf
        // and:
        // https://github.com/waveshare/e-Paper/blob/master/STM32/STM32-F103ZET6/User/e-Paper/EPD_1in54c.c
        self.interface.reset(delay, 2);

        self.cmd_with_data(spi, Command::UnknownInit, &[0x3f])?;

        // set the panel settings
        self.cmd_with_data(spi, Command::PanelSetting, &[0x6f])?;
        // power setting
        self.cmd_with_data(spi, Command::PowerSetting, &[0x03, 0x00, 0x2b, 0x2b])?;
        // charge pump
        self.cmd_with_data(spi, Command::ChargePumpSetting, &[0x3f])?;
        // set lut
        self.cmd_with_data(spi, Command::LutOpt, &[0x00, 0x00])?;
        // set clock 50Hz
        self.cmd_with_data(spi, Command::PllControl, &[0x17])?;
        // Set VCOM and data output interval
        self.cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x57])?;
        // Set The non-overlapping period of Gate and Source.
        self.cmd_with_data(spi, Command::GateAndSourceNonOverlapPeriod, &[0x22])?;
        // set resolution
        self.send_resolution(spi)?;
        // sets VCOM_DC value
        self.cmd_with_data(spi, Command::VcmDcSetting, &[0x12])?;
        // Set POWER SAVING
        self.cmd_with_data(spi, Command::PowerSaving, &[0x33])?;

        self.set_full_reg(spi)?;

        // power on
        self.command(spi, Command::PowerOn)?;
        delay.delay_ms(5);
        self.wait_until_idle();
        Ok(())
    }
}

impl<SPI, CS, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, CS, BUSY, DC, RST, DELAY>
    for Epd1in02<SPI, CS, BUSY, DC, RST, DELAY>
where
    SPI: Write<u8>,
    CS: OutputPin,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayMs<u8>,
{
    type DisplayColor = Color;
    fn new(
        spi: &mut SPI,
        cs: CS,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
    ) -> Result<Self, SPI::Error> {
        let interface = DisplayInterface::new(cs, busy, dc, rst);
        let color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = Epd1in02 { interface, color };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn sleep(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle();

        self.command(spi, Command::PowerOff)?;
        self.wait_until_idle();
        self.cmd_with_data(spi, Command::DeepSleep, &[0xa5])?;

        Ok(())
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn set_background_color(&mut self, color: Color) {
        self.color = color;
    }

    fn background_color(&self) -> &Color {
        &self.color
    }

    fn width(&self) -> u32 {
        WIDTH
    }

    fn height(&self) -> u32 {
        HEIGHT
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        // Clear the chromatic layer
        let color = self.color.get_byte_value();

        self.command(spi, Command::DataStartTransmission1)?;
        self.interface.data_x_times(spi, color, NUM_DISPLAY_BITS)?;
        self.wait_until_idle();
        self.cmd_with_data(spi, Command::DataStartTransmission2, buffer)?;
        Ok(())
    }

    #[allow(unused)]
    fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        unimplemented!()
    }

    fn display_frame(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::DisplayRefresh)?;
        self.wait_until_idle();

        Ok(())
    }

    fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.update_frame(spi, buffer, delay)?;
        self.display_frame(spi, delay)?;

        Ok(())
    }

    fn clear_frame(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle();
        let color = DEFAULT_BACKGROUND_COLOR.get_byte_value();

        // Clear the black
        self.command(spi, Command::DataStartTransmission1)?;
        self.interface.data_x_times(spi, color, NUM_DISPLAY_BITS)?;

        // Clear the chromatic
        self.command(spi, Command::DataStartTransmission2)?;
        self.interface.data_x_times(spi, color, NUM_DISPLAY_BITS)?;

        Ok(())
    }

    fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        Ok(())
    }

    fn is_busy(&self) -> bool {
        self.interface.is_busy(IS_BUSY_LOW)
    }
}

impl<SPI, CS, BUSY, DC, RST, DELAY> Epd1in02<SPI, CS, BUSY, DC, RST, DELAY>
where
    SPI: Write<u8>,
    CS: OutputPin,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayMs<u8>,
{
    fn command(&mut self, spi: &mut SPI, command: Command) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, command)
    }

    fn send_data(&mut self, spi: &mut SPI, data: &[u8]) -> Result<(), SPI::Error> {
        self.interface.data(spi, data)
    }

    fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data)
    }

    fn wait_until_idle(&mut self) {
        let _ = self.interface.wait_until_idle(IS_BUSY_LOW);
    }

    fn send_resolution(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        let w = self.width();
        let h = self.height();

        self.command(spi, Command::ResolutionSetting)?;

        // | D7 | D6 | D5 | D4 | D3 | D2 | D1 | D0 |
        // |    |     HRES[6:3]     |  0 |  0 |  0 |
        self.send_data(spi, &[(w as u8) & 0b0111_1000])?;
        // | D7 | D6 | D5 | D4 | D3 | D2 | D1 | D0 |
        // |               VRES[7:0]               |
        // Specification shows C/D is zero while sending the last byte,
        // but upstream code does not implement it like that. So for now
        // we follow upstream code.
        self.send_data(spi, &[h as u8])
    }

    fn set_full_reg(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> { 
        self.cmd_with_data(spi, Command::LutG0, &LUT_W1)?;
        self.cmd_with_data(spi, Command::LutG1, &LUT_B1)
    }
}
