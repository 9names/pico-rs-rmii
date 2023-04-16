#![no_std]
#![no_main]

use crate::{
    mdio::Mdio,
    pio::{init_eth, EthPins},
};

use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
mod clocks;
mod delay;
mod mdio;
mod pio;
use ieee802_3_miim::{phy::LAN8720A, Miim};
use panic_probe as _;
use rp2040_hal as hal;

use hal::{clocks::*, entry, pac, sio::Sio};

#[entry]
fn main() -> ! {
    println!("Program start");

    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let sio = Sio::new(pac.SIO);

    let clocks = clocks::setup_clocks(
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
        // Those 3 pins should be one after the other
        rx_d0: pins.gpio6.into(),
        rx_d1: pins.gpio7.into(),
        crs: pins.gpio8.into(),
        // Those 3 pins should be one after the other
        tx_d0: pins.gpio10.into(),
        tx_d1: pins.gpio11.into(),
        tx_en: pins.gpio12.into(),
    };
    let mut mdio = Mdio::new(pins.gpio14.into(), pins.gpio15.into());
    delay.delay_ms(1000);
    init_eth(eth_pins, pac.PIO0, pac.DMA, &mut pac.RESETS);
    delay.delay_ms(1000);

    // Retrieve the LAN8720A address
    let mut phy_address: Option<u8> = None;
    while phy_address.is_none() {
        debug!("searching for phy");
        for i in 0..32u8 {
            if mdio.read(i, 0) != 0xffff {
                phy_address = Some(i);
                break;
            }
        }
        delay.delay_us(1);
    }

    let phy_address = phy_address.expect("phy not found");
    defmt::info!("phy address {:?}", phy_address);
    let mut t = LAN8720A::new(mdio, phy_address);
    defmt::info!("Initialising phy");
    t.phy_init();
    defmt::info!("Blocking until link is up");
    t.block_until_link();
    defmt::info!("Link is up");

    let mut led_pin = pins.gpio25.into_push_pull_output();
    let mut last_link_up = false;
    let mut last_neg_done = false;
    loop {
        let link_up = t.link_established();
        if link_up != last_link_up {
            if link_up {
                defmt::info!("link up")
            } else {
                defmt::info!("link down")
            }
            last_link_up = link_up;
        }

        let speed = t.link_speed();
        let neg_done = speed.is_some();
        if neg_done != last_neg_done {
            if neg_done {
                let speed_str = if let Some(speed) = speed {
                    match speed {
                        ieee802_3_miim::phy::PhySpeed::HalfDuplexBase10T => "HalfDuplexBase10T",
                        ieee802_3_miim::phy::PhySpeed::FullDuplexBase10T => "FullDuplexBase10T",
                        ieee802_3_miim::phy::PhySpeed::HalfDuplexBase100Tx => "HalfDuplexBase100Tx",
                        ieee802_3_miim::phy::PhySpeed::FullDuplexBase100Tx => "FullDuplexBase100Tx",
                    }
                } else {
                    "Unknown"
                };
                defmt::info!("auto-negotiation complete, speed is {}", speed_str)
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

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;
