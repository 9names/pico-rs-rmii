use embedded_hal::digital::v2::{OutputPin, PinState, StatefulOutputPin};

use hal::gpio::{DynFunction, DynPin, DynPinMode};
use hal::pac;
use hal::pio::PIOExt;
use pio::SideSet;
use rp2040_hal as hal;

use crate::mdio::Mdio;

pub struct EthPins {
    pub ref_clk: DynPin,
    pub md_io: DynPin,
    pub md_clk: DynPin,
    pub tx_d0: DynPin,
    pub tx_d1: DynPin,
    pub tx_en: DynPin,
    pub rx_d0: DynPin,
    pub rx_d1: DynPin,
    pub crs: DynPin,
}

#[allow(dead_code)]
pub enum TargetPio {
    Pio0,
    Pio1,
}

impl EthPins {
    pub fn setup_pins(mut self, target_pio: TargetPio) -> EthPins {
        self = match target_pio {
            TargetPio::Pio0 => self.setup_pins_with_pio(DynFunction::Pio0),
            TargetPio::Pio1 => self.setup_pins_with_pio(DynFunction::Pio1),
        };
        self
    }

    fn setup_pins_with_pio(mut self, target_pio: DynFunction) -> EthPins {
        [
            &mut self.crs,
            &mut self.tx_en,
            &mut self.tx_d0,
            &mut self.tx_d1,
            &mut self.rx_d0,
            &mut self.rx_d1,
        ]
        .map(|mut pin| pin.try_into_mode(DynPinMode::Function(target_pio)).unwrap());

        self.ref_clk.into_push_pull_output();
        self.md_io.into_push_pull_output();
        self.md_clk.into_push_pull_output();
        self
    }
}

fn clear_dma_channel(dma: &pac::DMA, channel: u8) {
    dma.ch[channel as usize].ch_ctrl_trig.write(|w| {
        w.incr_read().set_bit();
        w.incr_write().clear_bit();
        w.treq_sel().permanent();
        unsafe {
            w.chain_to().bits(channel);
        }
        w.data_size().size_word();
        w.ring_sel().clear_bit();
        w.ring_size().ring_none();
        w.bswap().clear_bit();
        w.irq_quiet().clear_bit();
        w.en().set_bit();
        w.sniff_en().clear_bit();
        w
    });
}

fn setup_dma(dma: &pac::DMA) {
    let dma_rx_ch = 0;
    let dma_tx_ch = 1;
    [dma_rx_ch, dma_tx_ch].map(|channel| clear_dma_channel(&dma, channel));

    // Configure RX channel
    dma.ch[dma_rx_ch as usize].ch_ctrl_trig.write(|w| {
        w.incr_read().clear_bit();
        w.incr_write().set_bit();
        unsafe {
            w.treq_sel().bits(0);
        }
        w.data_size().size_byte();
        w
    });

    // Configure TX channel
    dma.ch[dma_tx_ch as usize].ch_ctrl_trig.write(|w| {
        w.incr_read().set_bit();
        w.incr_write().clear_bit();
        unsafe {
            w.treq_sel().bits(4 + 1);
        }
        w.data_size().size_byte();
        w
    });
}

pub fn init_eth(mut pins: EthPins, pio0: pac::PIO0, dma: pac::DMA, resets: &mut pac::RESETS) {
    let rx_d0_index = pins.rx_d0.id().num;
    let tx_d0_index = pins.tx_d0.id().num;

    pins = pins.setup_pins(TargetPio::Pio0);

    setup_dma(&dma);

    let mdio = Mdio::init(pins.md_io, pins.md_clk);

    let rx_program = {
        let mut a = pio::Assembler::<32>::new();
        let mut wrap_target = a.label();
        let mut wrap_source = a.label();

        // Wait PHY to be ready
        a.wait(0, pio::WaitSource::PIN, 2, false);
        a.wait(0, pio::WaitSource::PIN, 0, false);
        a.wait(0, pio::WaitSource::PIN, 1, false);
        a.wait(1, pio::WaitSource::PIN, 2, false);
        a.wait(1, pio::WaitSource::PIN, 0, false);
        a.wait(1, pio::WaitSource::PIN, 1, false);

        // Set the label for wrapping the program back to here
        a.bind(&mut wrap_target);

        // Retrieve data
        a.r#in(pio::InSource::PINS, 2);

        // Define end of program loop
        a.bind(&mut wrap_source);

        a.assemble_with_wrap(wrap_source, wrap_target)
    };

    let tx_program = {
        let mut a = pio::Assembler::<32>::new_with_side_set(SideSet::new(false, 1, false));
        let mut wrap_target = a.label();
        let mut wrap_source = a.label();
        let mut data_loop = a.label();

        // This program runs at half a cycle per clock (1HC/clock) to properly handle
        // the data loop, so every delay needs to be doubled

        // Set the label for wrapping the program back to here
        a.bind(&mut wrap_target);

        // Wait for data to transmit
        // TODO: sync on clock GPIO?
        a.pull_with_side_set(false, true, 0);

        // Write 0b01 for 31 cycles
        a.set_with_delay_and_side_set(pio::SetDestination::PINS, 0b01, 15, 1); // 16 HC
        a.nop_with_delay_and_side_set(15, 1); // 16 HC
        a.nop_with_delay_and_side_set(15, 1); // 16 HC
        a.nop_with_delay_and_side_set(13, 1); // 14 HC
                                              // Total: 62HC = 31 cycles

        // Write 0b11 for 1 cycle
        a.set_with_delay_and_side_set(pio::SetDestination::PINS, 0b11, 1, 1); // 2 HC = 1 cycle

        // Write the data, 2 bits at a time
        a.bind(&mut data_loop);
        a.out_with_side_set(pio::OutDestination::PINS, 2, 1); // 1 HC
        a.jmp_with_side_set(
            pio::JmpCondition::OutputShiftRegisterNotEmpty,
            &mut data_loop,
            1,
        ); // 1 HC
           // Total: 2HC = 1 cycle

        // Define end of program loop
        a.bind(&mut wrap_source);

        a.assemble_with_wrap(wrap_source, wrap_target)
    };

    // Initialize PIO
    let (mut pio, sm0, sm1, _, _) = pio0.split(resets);

    // Prepare SM for RX program
    let installed_rx = pio.install(&rx_program).unwrap();
    let (sm_rx, _, _) = hal::pio::PIOBuilder::from_program(installed_rx)
        .in_pin_base(rx_d0_index)
        .in_shift_direction(hal::pio::ShiftDirection::Right)
        .autopush(true)
        .push_threshold(8)
        .buffers(hal::pio::Buffers::OnlyRx)
        .clock_divisor_fixed_point(2, 0)
        .build(sm0);

    // Prepare SM for TX program
    let installed_tx = pio.install(&tx_program).unwrap();
    let (mut sm_tx, _, _) = hal::pio::PIOBuilder::from_program(installed_tx)
        .out_pins(tx_d0_index, 2)
        .side_set_pin_base(tx_d0_index + 2)
        .clock_divisor_fixed_point(1, 0)
        .build(sm1);

    sm_tx.set_pindirs([
        (tx_d0_index, hal::pio::PinDir::Output),
        (tx_d0_index + 1, hal::pio::PinDir::Output),
        (tx_d0_index + 2, hal::pio::PinDir::Output),
    ]);

    // Start both state machines
    sm_rx.start();
    sm_tx.start();
}
