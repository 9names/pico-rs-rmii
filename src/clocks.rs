use fugit::HertzU32;
use panic_probe as _;
use rp2040_hal as hal;

use hal::{
    clocks::*,
    pac,
    pll::{common_configs::PLL_USB_48MHZ, setup_pll_blocking, PLLConfig},
    watchdog::Watchdog,
    xosc::setup_xosc_blocking,
};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClockError {
    /// Failed to setup the external crystal oscillator
    Xosc,
    /// Failed to setup the System PLL
    PllSys,
    /// Failed to setup the USB PLL
    PllUsb,
    /// Failed to setup one of the following clocks to be fed by one of the system clock sources:
    /// reference_clock, system_clock, usb_clock, adc_clock, rtc_clock, peripheral_clock, gpio_output0_clock
    SubClock,
}

pub(crate) fn setup_clocks(
    xosc: pac::XOSC,
    pll_sys: pac::PLL_SYS,
    pll_usb: pac::PLL_USB,
    clocks: pac::CLOCKS,
    resets: &mut pac::RESETS,
    watchdog: pac::WATCHDOG,
) -> Result<ClocksManager, ClockError> {
    let xosc_crystal_freq = HertzU32::MHz(12);
    let xosc = setup_xosc_blocking(xosc, xosc_crystal_freq).map_err(|_| ClockError::Xosc)?;

    // Configure watchdog tick generation to tick over every microsecond
    let mut watchdog = Watchdog::new(watchdog);
    let watchdog_freq = HertzU32::MHz(1);
    watchdog.enable_tick_generation((xosc_crystal_freq / watchdog_freq) as u8);

    // External clock from RMII module
    let clock_gpio_freq = HertzU32::MHz(50);

    let mut clocks: ClocksManager = ClocksManager::new(clocks);

    pub const PLL_SYS_100MHZ: PLLConfig = PLLConfig {
        vco_freq: HertzU32::MHz(1500),
        refdiv: 1,
        post_div1: 5,
        post_div2: 3,
    };

    let pll_sys = setup_pll_blocking(
        pll_sys,
        xosc.operating_frequency(),
        PLL_SYS_100MHZ,
        &mut clocks,
        resets,
    )
    .map_err(|_| ClockError::PllSys)?;

    let pll_usb = setup_pll_blocking(
        pll_usb,
        xosc.operating_frequency(),
        PLL_USB_48MHZ,
        &mut clocks,
        resets,
    )
    .map_err(|_| ClockError::PllUsb)?;

    // CLK_REF = XOSC (12MHz) / 1 = 12MHz
    clocks
        .reference_clock
        .configure_clock(&xosc, xosc.get_freq())
        .map_err(|_| ClockError::SubClock)?;

    // CLK SYS = PLL SYS (100MHz) / 1 = 100MHz
    clocks
        .system_clock
        .configure_clock(&pll_sys, pll_sys.get_freq())
        .map_err(|_| ClockError::SubClock)?;

    // CLK USB = PLL USB (48MHz) / 1 = 48MHz
    clocks
        .usb_clock
        .configure_clock(&pll_usb, pll_usb.get_freq())
        .map_err(|_| ClockError::SubClock)?;

    // CLK ADC = PLL USB (48MHZ) / 1 = 48MHz
    clocks
        .adc_clock
        .configure_clock(&pll_usb, pll_usb.get_freq())
        .map_err(|_| ClockError::SubClock)?;

    // CLK RTC = PLL USB (48MHz) / 1024 = 46875Hz
    clocks
        .rtc_clock
        .configure_clock(&pll_usb, HertzU32::Hz(46875))
        .map_err(|_| ClockError::SubClock)?;

    // CLK PERI = clk_sys. Used as reference clock for Peripherals. No dividers so just select and enable
    // Normally choose clk_sys or clk_usb
    clocks
        .peripheral_clock
        .configure_clock(&clocks.system_clock, clock_gpio_freq)
        .map_err(|_| ClockError::SubClock)?;

    clocks
        .gpio_output0_clock
        .configure_clock(&clocks.system_clock, clock_gpio_freq)
        .map_err(|_| ClockError::SubClock)?;

    Ok(clocks)
}
