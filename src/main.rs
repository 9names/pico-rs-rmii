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
mod lan8720a;
mod mdio;
mod pio;
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

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;
