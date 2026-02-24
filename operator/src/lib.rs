use crate::operator::GeneralOperator;
use shared::{self, ipc::events::Event, operator::Operator, texture::set_texture_cb};
use tracing::debug;
mod operator;
mod skin;

#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn new(
    id: *const std::ffi::c_char,
    event_tx_ptr: *const std::ffi::c_void,
) -> Result<Box<dyn Operator>, shared::operator::Error> {
    shared::logging::init_logger().expect("failed to init logger");
    set_texture_cb();

    let op_id = if id.is_null() {
        None
    } else {
        unsafe { Some(std::ffi::CStr::from_ptr(id).to_string_lossy().into_owned()) }
    }
    .ok_or(shared::operator::Error::InvalidOperatorId("".to_string()))?;

    let event_tx = unsafe { (*(event_tx_ptr as *const tokio::sync::mpsc::Sender<Event>)).clone() };

    debug!(
        "creating operator: {:?} using channel, {:?}",
        op_id, event_tx
    );
    Ok(GeneralOperator::new(&op_id, event_tx)?)
}
