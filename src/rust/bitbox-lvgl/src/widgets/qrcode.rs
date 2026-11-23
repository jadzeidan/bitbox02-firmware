// SPDX-License-Identifier: Apache-2.0

use core::ffi::c_void;
use core::ptr::NonNull;

use super::canvas::{CanvasExt, LvCanvas};
use crate::{LvHandle, LvObj, class, ffi};

pub type LvQrcode = LvHandle<class::QrcodeTag>;

/// The data does not fit into a QR code of the widget's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LvQrcodeUpdateError;

pub trait QrcodeExt: CanvasExt {
    /// Encodes `data` in byte mode with medium error correction. The modules are scaled by the
    /// largest whole factor that fits the widget's size, and the code is centered.
    fn update(&self, data: &[u8]) -> Result<(), LvQrcodeUpdateError> {
        let len = u32::try_from(data.len()).map_err(|_| LvQrcodeUpdateError)?;
        match unsafe { ffi::lv_qrcode_update(self.as_ptr(), data.as_ptr() as *const c_void, len) } {
            ffi::lv_result_t::LV_RESULT_OK => Ok(()),
            _ => Err(LvQrcodeUpdateError),
        }
    }
}

impl LvHandle<class::QrcodeTag> {
    pub fn new<P: class::LvClass>(parent: &LvHandle<P>) -> Option<Self> {
        NonNull::new(unsafe { ffi::lv_qrcode_create(parent.as_ptr()) }).map(LvHandle::from_ptr)
    }

    /// Sets the width and height of the square bitmap the code is drawn into.
    pub fn set_size(&self, size: i32) {
        unsafe { ffi::lv_qrcode_set_size(self.as_ptr(), size) }
    }

    pub fn to_canvas(self) -> LvCanvas {
        self.cast()
    }

    pub fn to_obj(self) -> LvObj {
        self.cast()
    }
}

impl QrcodeExt for LvHandle<class::QrcodeTag> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ObjExt;

    #[test]
    fn test_update() {
        let _lock = crate::test_util::lock_and_init();

        let display = crate::LvDisplay::new(16, 16).unwrap();
        let screen = display.screen_active().unwrap();
        let qrcode = LvQrcode::new(&screen).unwrap();
        // "abc" needs the smallest code, 21 modules: 2px per module with a 4px margin.
        qrcode.set_size(50);
        qrcode.update(b"abc").unwrap();

        let draw_buf = unsafe { qrcode.get_draw_buf().unwrap().as_ref() };
        let stride = draw_buf.header.stride() as usize;
        // One bit per pixel after the two-color palette; a set bit is a dark pixel.
        let is_dark = |x: usize, y: usize| {
            let byte = unsafe { *draw_buf.data.add(8 + y * stride + x / 8) };
            byte & (0x80 >> (x % 8)) != 0
        };
        let is_module_dark = |column: usize, row: usize| is_dark(4 + 2 * column, 4 + 2 * row);
        assert!(!is_dark(3, 3));
        // The finder pattern in the top left corner: a dark ring around a dark center.
        assert!(is_module_dark(0, 0));
        assert!(is_module_dark(6, 6));
        assert!(!is_module_dark(1, 1));
        assert!(is_module_dark(3, 3));
        assert!(!is_module_dark(7, 7));

        // 300 bytes need more than 50 modules per side.
        assert_eq!(qrcode.update(&[0; 300]), Err(LvQrcodeUpdateError));

        unsafe { qrcode.delete() };
    }
}
