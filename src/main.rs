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
    );
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
    for i in 0..32 {
        if mdio.read(i, 0, &mut delay) != 0xffff {
            phy_address = Some(i as u8);
            break;
        }
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
    loop {
        let mdio_status = mdio.read(phy_address, lan8720a::BASIC_STATUS_REG, &mut delay);
        defmt::info!("mdio status {:X}", mdio_status);

        if (mdio_status & lan8720a::BASIC_STATUS_REG_LINK_STATUS) != 0 {
            defmt::info!("link up")
        } else {
            defmt::info!("link down")
        }

        if (mdio_status & lan8720a::BASIC_STATUS_REG_AUTO_NEGO_COMPLETE) != 0 {
            defmt::info!("auto-negotiation complete")
        } else {
            defmt::info!("auto-negotiation not yet done")
        }

        info!("on!");
        led_pin.set_high().unwrap();
        delay.delay_ms(500);
        info!("off!");
        led_pin.set_low().unwrap();
        delay.delay_ms(500);
    }
}

fn setup_clocks(
    xosc: pac::XOSC,
    pll_sys: pac::PLL_SYS,
    pll_usb: pac::PLL_USB,
    clocks: pac::CLOCKS,
    resets: &mut pac::RESETS,
    watchdog: pac::WATCHDOG,
) -> ClocksManager {
    let xosc_crystal_freq = HertzU32::MHz(12);
    let xosc = setup_xosc_blocking(xosc, xosc_crystal_freq).unwrap_or_else(|_| {
        error!("Xosc failed to start");
        loop {}
    });

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
        xosc.operating_frequency().into(),
        PLL_SYS_100MHZ,
        &mut clocks,
        resets,
    )
    .unwrap_or_else(|_| {
        error!("Failed to start SYS PLL");
        // led_pin.set_high().unwrap();
        loop {}
    });

    let pll_usb = setup_pll_blocking(
        pll_usb,
        xosc.operating_frequency().into(),
        PLL_USB_48MHZ,
        &mut clocks,
        resets,
    )
    .unwrap_or_else(|_| {
        error!("Failed to start USB PLL");
        loop {}
    });

    let clocks = (|| {
        // CLK_REF = XOSC (12MHz) / 1 = 12MHz
        clocks
            .reference_clock
            .configure_clock(&xosc, xosc.get_freq())?;

        // CLK SYS = PLL SYS (100MHz) / 1 = 100MHz
        clocks
            .system_clock
            .configure_clock(&pll_sys, pll_sys.get_freq())?;

        // CLK USB = PLL USB (48MHz) / 1 = 48MHz
        clocks
            .usb_clock
            .configure_clock(&pll_usb, pll_usb.get_freq())?;

        // CLK ADC = PLL USB (48MHZ) / 1 = 48MHz
        clocks
            .adc_clock
            .configure_clock(&pll_usb, pll_usb.get_freq())?;

        // CLK RTC = PLL USB (48MHz) / 1024 = 46875Hz
        clocks
            .rtc_clock
            .configure_clock(&pll_usb, HertzU32::Hz(46875))?;

        // CLK PERI = clk_sys. Used as reference clock for Peripherals. No dividers so just select and enable
        // Normally choose clk_sys or clk_usb
        clocks
            .peripheral_clock
            .configure_clock(&clocks.system_clock, clock_gpio_freq)?;

        clocks
            .gpio_output0_clock
            .configure_clock(&clocks.system_clock, clock_gpio_freq)?;

        Ok(clocks)
    })()
    .unwrap_or_else(|_: ClockError| {
        error!("Failed to set clocks");
        loop {}
    });

    clocks
}

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;
