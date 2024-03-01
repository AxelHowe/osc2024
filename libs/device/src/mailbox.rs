use library::sync::mutex::Mutex;
use tock_registers::{
    interfaces::{Readable, Writeable},
    register_bitfields, register_structs,
    registers::{ReadOnly, WriteOnly},
};

use crate::{device_driver::DeviceDriver, mmio_deref_wrapper::MMIODerefWrapper};

#[repr(u32)]
enum BufferRequestCode {
    ProcessRequest = 0,
}

#[repr(u32)]
enum BufferResponseCode {
    RequestSuccessful = 0x80000000,
    ErrorParsingRequestBuffer = 0x80000001,
}

#[repr(u32)]
enum TagIdentifier {
    GetBoardRevision = 0x00010002,
}

register_bitfields! [
    u32,
    MAILBOX_STATUS [
        FULL OFFSET(31) NUMBITS(1) [
            NOT_FULL = 0,
            FULL = 1,
        ],
        EMPTY OFFSET(30) NUMBITS(1) [
            NOT_EMPTY = 0,
            EMPTY = 1,
        ]
    ],
    MAILBOX_READ [
        DATA OFFSET(4) NUMBITS(28) [],
        CHANNEL OFFSET(0) NUMBITS(4) []
    ],
    MAILBOX_WRITE [
        DATA OFFSET(4) NUMBITS(28) [],
        CHANNEL OFFSET(0) NUMBITS(4) []
    ]
];

register_structs! {
    Registers {
        (0x00 => read: ReadOnly<u32, MAILBOX_READ::Register>),
        (0x04 => _reserved1),
        (0x18 => status: ReadOnly<u32, MAILBOX_STATUS::Register>),
        (0x1c => _reserved2),
        (0x20 => write: WriteOnly<u32, MAILBOX_WRITE::Register>),
        (0x24 => @END),
    }
}

struct MailboxInner {
    registers: MMIODerefWrapper<Registers>,
}

impl MailboxInner {
    /**
     * # Safety
     *
     * - The user must ensure to provide a correct MMIO start address.
     */
    const unsafe fn new(mmio_start_addr: usize) -> Self {
        Self {
            registers: MMIODerefWrapper::new(mmio_start_addr),
        }
    }

    fn is_writable(&self) -> bool {
        !self.registers.status.is_set(MAILBOX_STATUS::FULL)
    }

    fn is_readable(&self) -> bool {
        !self.registers.status.is_set(MAILBOX_STATUS::EMPTY)
    }

    fn read(&self, channel: u8) -> *mut u32 {
        loop {
            while !self.is_readable() {}
            let tmp = self.registers.read.get();
            let data = tmp & !(0b1111);
            let data_channel = (tmp & 0b1111) as u8;
            if data_channel == channel {
                // get 28 MSB
                return data as *mut u32;
            }
        }
    }

    fn write(&self, channel: u8, buffer_addr: *mut u32) {
        while !self.is_writable() {}
        // use 28 MSB
        let message_addr = buffer_addr as u32 & !(0b1111);
        self.registers.write.set(message_addr | channel as u32);
    }

    fn call(&self, buffer_addr: *mut u32) -> *mut u32 {
        self.write(8, buffer_addr);
        self.read(8)
    }

    fn get_board_revision(&self) -> u32 {
        #[repr(align(16))]
        struct GetBoardRevisionBuffer {
            inner: [u32; 7],
        }
        let mut buffer = GetBoardRevisionBuffer { inner: [0; 7] };
        // set buffer length (in bytes)
        buffer.inner[0] = 7 * 4;
        // set request code
        buffer.inner[1] = BufferRequestCode::ProcessRequest as u32;
        // set tag tag identifier
        buffer.inner[2] = TagIdentifier::GetBoardRevision as u32;
        // set value buffer length (in bytes)
        buffer.inner[3] = 4;
        // set tag request code for request
        buffer.inner[4] = 0;
        // set value buffer
        buffer.inner[5] = 0;
        // set end tag bits
        buffer.inner[6] = 0;
        self.call(buffer.inner.as_mut_ptr());
        buffer.inner[5]
    }
}

pub struct Mailbox {
    inner: Mutex<MailboxInner>,
}

impl Mailbox {
    /**
     * # Safety
     *
     * - The user must ensure to provide a correct MMIO start address.
     */
    pub const unsafe fn new(mmio_start_addr: usize) -> Self {
        Self {
            inner: Mutex::new(MailboxInner::new(mmio_start_addr)),
        }
    }

    pub fn get_board_revision(&self) -> u32 {
        self.inner.lock().unwrap().get_board_revision()
    }
}

impl DeviceDriver for Mailbox {}
