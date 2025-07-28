use std::process::exit;

use crate::internal::user_interface::colors::StringColor;
use crate::omni_info;

pub fn self_update(force: bool) {
    if force {
        omni_info!("self-update is disabled for this build");
        exit(1);
    }
}
