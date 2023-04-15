#![no_std]
#![no_main]

use crate::{
    mdio::Mdio,
    pio::{init_eth, EthPins},
};

use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use fugit::HertzU32;
mod lan8720a;
mod mdio;
mod pio;
use panic_probe as _;
use rp2040_hal as hal;

use hal::{
    clocks::*,
    entry, pac,
    pll::{common_configs::PLL_USB_48MHZ, setup_pll_blocking, PLLConfig},
    sio::Sio,
    watchdog::Watchdog,
    xosc::setup_xosc_blocking,
};

#[entry]
fn main() -> ! {
    println!("Program start");

    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let sio = Sio::new(pac.SIO);

    let clocks = setup_clocks(
        pac.XOSC,
        pac.PLL_SYS,
        pac.PLL_USB,
        pac.CLOCKS,
        &mut pac.RESETS,
        pac.WATCHDOG,
    )
    .expect("Failed to configure clocks");
    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let eth_pins = EthPins {
        ref_clk: pins.gpio21.into(),
        md_io: pins.gpio16.into(),
        md_clk: pins.gpio17.into(),
        // Those 3 pins should be one after the other
        rx_d0: pins.gpio6.into(),
        rx_d1: pins.gpio7.into(),
        crs: pins.gpio8.into(),
        // Those 3 pins should be one after the other
        tx_d0: pins.gpio10.into(),
        tx_d1: pins.gpio11.into(),
        tx_en: pins.gpio12.into(),
    };
    let mut mdio = Mdio::init(pins.gpio14.into(), pins.gpio15.into());
    delay.delay_ms(1000);
    init_eth(eth_pins, pac.PIO0, pac.DMA, &mut pac.RESETS);
    delay.delay_ms(1000);
    // Retrieve the LAN8720A address
    let mut phy_address: Option<u8> = None;
    while phy_address.is_none() {
        debug!("searching for phy");
        for i in 0..32u8 {
            if mdio.read(i, 0, &mut delay) != 0xffff {
                phy_address = Some(i);
                break;
            }
        }
        delay.delay_us(1);
    }

    let phy_address = phy_address.expect("phy not found");
    mdio.write(
        phy_address,
        lan8720a::AUTO_NEGO_REG,
        lan8720a::AUTO_NEGO_REG_IEEE802_3
            | lan8720a::AUTO_NEGO_REG_100_ABI
            | lan8720a::AUTO_NEGO_REG_100_FD_ABI,
        &mut delay,
    );
    mdio.write(phy_address, lan8720a::BASIC_CONTROL_REG, 0x1000, &mut delay);
    defmt::info!("phy address {:?}", phy_address);

    let mut led_pin = pins.gpio25.into_push_pull_output();
    let mut last_link_up = false;
    let mut last_neg_done = false;
    loop {
        let mdio_status = mdio.read(phy_address, lan8720a::BASIC_STATUS_REG, &mut delay);
        defmt::debug!("mdio status {:X}", mdio_status);

        let link_up = (mdio_status & lan8720a::BASIC_STATUS_REG_LINK_STATUS) != 0;
        if link_up != last_link_up {
            if link_up {
                defmt::info!("link up")
            } else {
                defmt::info!("link down")
            }
            last_link_up = link_up;
        }

        let neg_done = (mdio_status & lan8720a::BASIC_STATUS_REG_AUTO_NEGO_COMPLETE) != 0;
        if neg_done != last_neg_done {
            if neg_done {
                defmt::info!("auto-negotiation complete")
            } else {
                defmt::info!("auto-negotiation not yet done")
            }
            last_neg_done = neg_done;
        }

        led_pin.set_high().unwrap();
        delay.delay_ms(500);
        led_pin.set_low().unwrap();
        delay.delay_ms(500);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClockError {
    InvalidClock,
    XoscStartErr,
    PllSysErr,
    PllUsbErr,
    SubClockConfigErr,
}

fn setup_clocks(
    xosc: pac::XOSC,
    pll_sys: pac::PLL_SYS,
    pll_usb: pac::PLL_USB,
    clocks: pac::CLOCKS,
    resets: &mut pac::RESETS,
    watchdog: pac::WATCHDOG,
) -> Result<ClocksManager, ClockError> {
    let xosc_crystal_freq = HertzU32::MHz(12);
    let xosc =
        setup_xosc_blocking(xosc, xosc_crystal_freq).map_err(|_| ClockError::XoscStartErr)?;

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
    .map_err(|_| ClockError::PllSysErr)?;

    let pll_usb = setup_pll_blocking(
        pll_usb,
        xosc.operating_frequency(),
        PLL_USB_48MHZ,
        &mut clocks,
        resets,
    )
    .map_err(|_| ClockError::PllUsbErr)?;

    // CLK_REF = XOSC (12MHz) / 1 = 12MHz
    clocks
        .reference_clock
        .configure_clock(&xosc, xosc.get_freq())
        .map_err(|_| ClockError::SubClockConfigErr)?;

    // CLK SYS = PLL SYS (100MHz) / 1 = 100MHz
    clocks
        .system_clock
        .configure_clock(&pll_sys, pll_sys.get_freq())
        .map_err(|_| ClockError::SubClockConfigErr)?;

    // CLK USB = PLL USB (48MHz) / 1 = 48MHz
    clocks
        .usb_clock
        .configure_clock(&pll_usb, pll_usb.get_freq())
        .map_err(|_| ClockError::SubClockConfigErr)?;

    // CLK ADC = PLL USB (48MHZ) / 1 = 48MHz
    clocks
        .adc_clock
        .configure_clock(&pll_usb, pll_usb.get_freq())
        .map_err(|_| ClockError::SubClockConfigErr)?;

    // CLK RTC = PLL USB (48MHz) / 1024 = 46875Hz
    clocks
        .rtc_clock
        .configure_clock(&pll_usb, HertzU32::Hz(46875))
        .map_err(|_| ClockError::SubClockConfigErr)?;

    // CLK PERI = clk_sys. Used as reference clock for Peripherals. No dividers so just select and enable
    // Normally choose clk_sys or clk_usb
    clocks
        .peripheral_clock
        .configure_clock(&clocks.system_clock, clock_gpio_freq)
        .map_err(|_| ClockError::SubClockConfigErr)?;

    clocks
        .gpio_output0_clock
        .configure_clock(&clocks.system_clock, clock_gpio_freq)
        .map_err(|_| ClockError::SubClockConfigErr)?;

    Ok(clocks)
}

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;
