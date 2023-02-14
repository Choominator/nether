//! Pixel valve driver.
//!
//! Pixel valves are intended to be driven by the Hardware Video Scaler so they
//! are not officially documented, however they provide important event
//! notifications such as when the vertical synchronization signal is sent on
//! each port, and as such they are also important to software running on the
//! ARM cores.  My source of information regarding these devices is the
//! excellent work done by the community in reverse engineering the Video Core
//! in the [VPU Open Firmware project [1][2].
//!
//! [1]: https://github.com/librerpi/rpi-open-firmware/blob/master/docs/pixelvalve.md
//! [2]: https://github.com/librerpi/rpi-open-firmware/blob/master/docs/pixelvalve.txt

extern crate alloc;

use alloc::vec::Vec;

use crate::irq::IRQ;
use crate::sync::{Lazy, Lock};
use crate::PERRY_RANGE;

/// Pixel valve 1 IRQ.
const PV1_IRQ: u32 = 142;
/// Pixel valve 1 base address.
const PV1_BASE: usize = 0x2207000 + PERRY_RANGE.start;
/// Pixel valve 1 interrupt enable register.
const PV1_INTEN: *mut u32 = (PV1_BASE + 0x24) as _;
/// Pixel valve 1 status and acknowledgement register.
const PV1_STAT: *mut u32 = (PV1_BASE + 0x28) as _;
/// Pixel valve VSync interrupt flag.
const PV_VSYNC: u32 = 0x10;

/// Pixel valve 1 global driver instance.
pub static PIXVALVE: Lazy<PixelValve> = Lazy::new(PixelValve::new);

/// Pixel Valve driver.
#[derive(Debug)]
pub struct PixelValve
{
    /// Vertical synchronization event handlers.
    vsync_hdlrs: Lock<Vec<fn()>>,
    /// Vertical synchronization event handlers scheduled to be added to the
    /// event handlers list.
    vsync_new_hdlrs: Lock<Vec<fn()>>,
}

impl PixelValve
{
    /// Creates and initializes a new pixel valve driver instance.
    ///
    /// Returns the newly created driver instance.
    fn new() -> Self
    {
        IRQ.register(PV1_IRQ, Self::vsync);
        unsafe {
            PV1_STAT.write_volatile(PV_VSYNC);
            let evs = PV1_INTEN.read_volatile();
            PV1_INTEN.write_volatile(evs | PV_VSYNC);
        }
        Self { vsync_hdlrs: Lock::new(Vec::new()),
               vsync_new_hdlrs: Lock::new(Vec::new()) }
    }

    /// Schedules the registration of a handler for the vertical synchronization
    /// event.
    ///
    /// * `hdlr`: Handler function.
    pub fn register_vsync(&self, hdlr: fn())
    {
        self.vsync_new_hdlrs.lock().push(hdlr);
    }

    /// Dispatches the vertical synchronization event to all the registered
    /// handlers.
    fn vsync()
    {
        if unsafe { PV1_STAT.read_volatile() } & PV_VSYNC == 0 {
            return;
        }
        unsafe { PV1_STAT.write_volatile(PV_VSYNC) };
        // Append all scheduled handlers to the handler list.  Doing it this way avoids
        // a potential deadlock if a handler tries to schedule another handler, and also
        // avoids unnecessary memory allocations and deallocations that would result
        // from cloning and dropping the handlers list on every event.
        let mut hdlrs = PIXVALVE.vsync_hdlrs.lock();
        let mut new_hdlrs = PIXVALVE.vsync_new_hdlrs.lock();
        hdlrs.append(&mut *new_hdlrs);
        drop(new_hdlrs);
        hdlrs.iter().for_each(|hdlr| hdlr());
    }
}
