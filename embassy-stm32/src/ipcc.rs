use crate::pac::IPCC as PAC_IPCC;
use crate::peripherals::IPCC;
use crate::rcc::sealed::RccPeripheral;

use crate::interrupt;

use cortex_m::peripheral::NVIC;
use embassy::interrupt::Interrupt;
use embassy::interrupt::InterruptExt;
use embassy::util::Unborrow;
use embassy::waitqueue::AtomicWaker;
use embassy_hal_common::unborrow;

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

const NUM_IPCC_CHANNLES: usize = 6;

pub struct MyIpcc {
    ipcc: crate::peripherals::IPCC,
}

struct State {
    rx_waker: AtomicWaker,
    tx_waker: AtomicWaker,
}

impl State {
    const fn new() -> Self {
        State {
            rx_waker: AtomicWaker::new(),
            tx_waker: AtomicWaker::new(),
        }
    }
}

static STATE: State = State::new();

impl MyIpcc {
    pub fn new(
        peripheral: impl Unborrow<Target = IPCC>,
        //    tx_interrupt: impl Unborrow<Target = crate::interrupt::IPCC_C1_TX>,
        //    rx_interrupt: impl Unborrow<Target = crate::interrupt::IPCC_C1_RX>,
    ) -> Self {
        unborrow!(peripheral); //, tx_interrupt, rx_interrupt);
        IPCC::enable();

        IPCC::reset();

        // TODO: Handle interrupts

        // Enable RX interrupt
        unsafe {
            PAC_IPCC.cpu(0).cr().modify(|r| r.set_rxoie(true));
        }

        // Enable RX interrupt
        let rx_irq = unsafe { crate::interrupt::IPCC_C1_RX::steal() };

        rx_irq.unpend();
        rx_irq.enable();

        let instance = Self { ipcc: peripheral };

        instance
    }

    pub fn reset(&mut self) {
        unsafe {
            for cpu in 0..2 {
                // Clear RX flag
                PAC_IPCC.cpu(cpu).scr().write(|r| {
                    for chan in 0..NUM_IPCC_CHANNLES {
                        r.set_chc(chan, true);
                    }
                });

                // Mask channel occupied interrupt
                PAC_IPCC.cpu(cpu).mr().modify(|r| {
                    for chan in 0..NUM_IPCC_CHANNLES {
                        r.set_chom(chan, true);
                    }
                });
            }
        }
    }

    /// Mark channel as occupied. Indicates that valid data is present
    /// and that the other CPU can read data.
    pub fn set_channel_occupied(&mut self, channel: usize) {
        unsafe {
            PAC_IPCC.cpu(0).scr().write(|r| r.set_chs(channel, true));
        }
    }

    /// Enable interrupt for channel free. Interrupt is triggered as soon
    /// as the other CPU indicates that it has read the data.
    pub fn enable_channel_free_interrupt(&mut self, channel: usize) {
        unsafe {
            PAC_IPCC.cpu(0).mr().modify(|r| r.set_chfm(channel, false));
        }
    }

    pub fn disable_channel_free_interrupt(&mut self, channel: usize) {
        unsafe {
            PAC_IPCC.cpu(0).mr().modify(|r| r.set_chfm(channel, true));
        }
    }

    /// A half-duplex transfer over IPCC.
    ///
    /// This handles just the handshake part, the data needs to be written
    /// into a buffer separately.
    ///
    /// For a half-duplex transfer, the channel is marked as occupied,
    /// and then the other CPU can read the data. Each sent command
    /// requires a response, so the other CPU will write the response into the
    /// buffer, and then this function will return.
    pub async fn write_half_duplex(&mut self, channel: usize) {
        self.set_channel_occupied(channel);
        self.enable_channel_free_interrupt(channel);

        wait_for_half_duplex_transfer(channel).await;

        self.disable_channel_free_interrupt(channel);
    }

    /// Wait for an event on the given channel.
    ///
    /// Events are received in simplex mode from CPU 1.
    pub fn wait_for_event(&mut self, channel: usize) -> impl Future<Output = ()> {
        self.enable_irq_rx_for_channel(channel);

        EventRead { channel }
    }

    /// Eanble
    fn enable_irq_rx_for_channel(&mut self, channel: usize) {
        unsafe { PAC_IPCC.cpu(0).mr().modify(|r| r.set_chom(channel, false)) };
    }

    /// To receive events, simplex mode is used.
    pub fn event_pending(&self, channel: usize) -> bool {
        unsafe { PAC_IPCC.cpu(1).sr().read().chf(channel) }
    }
}

fn wait_for_half_duplex_transfer(channel: usize) -> impl Future<Output = ()> {
    HalfDuplexTransfer { channel }
}

struct HalfDuplexTransfer {
    channel: usize,
}

impl HalfDuplexTransfer {
    fn is_channel_occupied(&self) -> bool {
        unsafe { PAC_IPCC.cpu(0).sr().read().chf(self.channel) }
    }
}

impl Future for HalfDuplexTransfer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        STATE.rx_waker.register(cx.waker());

        // Check if channel is occupeid
        if self.is_channel_occupied() {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

struct EventRead {
    channel: usize,
}

impl EventRead {
    fn received_event(&self) -> bool {
        unsafe { PAC_IPCC.cpu(1).sr().read().chf(self.channel) }
    }
}

impl Future for EventRead {
    // TODO: Proper type
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        STATE.tx_waker.register(cx.waker());

        defmt::debug!("Polling future");

        // Check if we already received an event
        if self.received_event() {
            Poll::Ready(())
        } else {
            // TODO: Enable interrupt here?
            Poll::Pending
        }
    }
}

#[interrupt]
unsafe fn IPCC_C1_TX() {
    // TODO: Wakeup proper channel

    STATE.rx_waker.wake()
}

#[interrupt]
unsafe fn IPCC_C1_RX() {
    // TODO: Notify appropriate channel / event
    defmt::debug!("Got a RX interrupt!");

    // TODO: Properly detect channels here

    // Mask interrupt again
    unsafe { PAC_IPCC.cpu(0).mr().modify(|r| r.set_chom(1, true)) };

    // TODO: Use waker
    STATE.tx_waker.wake()
}
