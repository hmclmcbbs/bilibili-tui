//! Authentication API types

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct QrcodeData {
    pub url: String,
    pub qrcode_key: String,
}

#[derive(Debug, Deserialize)]
pub struct QrcodePollData {
    pub url: String,
    pub refresh_token: String,
    pub timestamp: i64,
    pub code: i32,
    pub message: String,
}

/// QR code poll status codes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QrcodePollStatus {
    /// Waiting for scan (86101)
    Waiting,
    /// Scanned, waiting for confirmation (86090)
    Scanned,
    /// Login successful (0)
    Success,
    /// QR code expired (86038)
    Expired,
    /// Unknown status
    Unknown(i32),
}

impl From<i32> for QrcodePollStatus {
    fn from(code: i32) -> Self {
        match code {
            86101 => QrcodePollStatus::Waiting,
            86090 => QrcodePollStatus::Scanned,
            0 => QrcodePollStatus::Success,
            86038 => QrcodePollStatus::Expired,
            _ => QrcodePollStatus::Unknown(code),
        }
    }
}

pub struct QrcodePollResult {
    pub data: Option<QrcodePollData>,
    pub cookies: Vec<(String, String)>,
}

/// Currently logged-in user profile (from `/x/web-interface/nav`).
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub mid: i64,
    pub uname: String,
    pub face: String,
    pub level: i32,
    /// Exp value at the start of the current level (`level_info.current_min`).
    pub current_min: i64,
    /// Current accumulated exp (`level_info.current_exp`).
    pub current_exp: i64,
    /// Exp needed to reach the next level (`level_info.next_exp`).
    pub next_exp: i64,
    /// Big member (大会员) status: 0 = not a member, 1 = active.
    pub vip_status: i32,
    /// Big member type: 0 = none, 1 = monthly, 2 = annual.
    pub vip_type: i32,
    /// Big member due date (unix seconds); 0 when not a member.
    pub vip_due_date: i64,
}
