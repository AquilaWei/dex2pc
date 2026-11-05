const FULL_ADDRESS: &str = "請輸入完整 IP:埠，例如 192.168.18.59:44517.";

#[derive(Default)]
pub(super) struct WifiFields {
    pub ip: String,
    pub editing: bool,
    pub port: String,
}

impl WifiFields {
    pub fn from_saved(saved: Option<&str>) -> Self {
        Self {
            ip: host(saved).unwrap_or_default().into(),
            editing: false,
            port: saved
                .and_then(|value| value.split_once(':'))
                .map(|(_, port)| port)
                .unwrap_or_default()
                .into(),
        }
    }
}

pub(super) fn host(saved: Option<&str>) -> Option<&str> {
    saved.and_then(|value| value.split_once(':').map(|(host, _)| host))
}

// Both connect and pair use the same visible IP; without one, request a full address.
pub(super) fn endpoint(input: &str, host: Option<&str>) -> Result<String, String> {
    match host {
        Some(host) => crate::wifi::validate(&format!("{}:{}", host.trim(), input.trim())),
        None if !input.trim().is_empty()
            && input.trim().bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            Err(FULL_ADDRESS.into())
        }
        None => crate::wifi::validate(input.trim()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_port_builds_saved_ip_address() {
        assert_eq!(
            endpoint("5555", host(Some("192.168.18.59:37001"))),
            Ok("192.168.18.59:5555".into())
        );
    }
    #[test]
    fn pairing_port_builds_saved_ip_address() {
        assert_eq!(
            endpoint("44517", host(Some("192.168.18.59:5555"))),
            Ok("192.168.18.59:44517".into())
        );
    }
    #[test]
    fn no_saved_ip_accepts_full_address() {
        assert_eq!(
            endpoint("192.168.18.59:37001", host(None)),
            Ok("192.168.18.59:37001".into())
        );
    }
    #[test]
    fn no_saved_ip_rejects_port_only() {
        assert_eq!(
            endpoint("37001", host(None)),
            Err("請輸入完整 IP:埠，例如 192.168.18.59:44517.".into())
        );
    }

    #[test]
    fn zero_port_is_rejected() {
        assert_eq!(
            endpoint("0", Some("192.168.18.59")),
            Err(
                "無效位址 \"192.168.18.59:0\"：請使用 IPv4 或主機名:埠號（1–65535，不支援 IPv6）"
                    .into()
            )
        );
    }
    #[test]
    fn port_above_65535_is_rejected() {
        assert_eq!(endpoint("65536", Some("192.168.18.59")), Err("無效位址 \"192.168.18.59:65536\"：請使用 IPv4 或主機名:埠號（1–65535，不支援 IPv6）".into()));
    }
    #[test]
    fn saved_ip_field_rejects_a_full_address_in_the_port_field() {
        assert_eq!(
            endpoint("192.168.18.59:44517", Some("192.168.12.137")),
            Err("無效位址 \"192.168.12.137:192.168.18.59:44517\"：請使用 IPv4 或主機名:埠號（1–65535，不支援 IPv6）".into())
        );
    }
}
