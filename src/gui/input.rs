const FULL_ADDRESS: &str = "請輸入完整 IP:埠，例如 192.168.18.59:44517.";

// Port-only input is a GUI convenience; the CLI validator remains strict.
pub(super) fn address(input: &str, other: &str, saved: Option<&str>) -> Result<String, String> {
    let input = input.trim();
    if !input.is_empty() && input.bytes().all(|byte| byte.is_ascii_digit()) {
        let host_source = if other.trim().is_empty() {
            saved.unwrap_or_default()
        } else {
            other.trim()
        };
        let host = host_source
            .split_once(':')
            .map(|(host, _)| host)
            .filter(|host| !host.is_empty())
            .ok_or_else(|| FULL_ADDRESS.to_owned())?;
        return crate::wifi::validate(&format!("{host}:{input}"));
    }
    crate::wifi::validate(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_port_uses_the_connect_field_host() {
        assert_eq!(
            address("44517", "192.168.18.59:5555", Some("192.168.12.137:5555")),
            Ok("192.168.18.59:44517".into())
        );
    }
    #[test]
    fn connect_port_uses_the_pairing_field_host() {
        assert_eq!(
            address("5555", "192.168.18.59:44517", None),
            Ok("192.168.18.59:5555".into())
        );
    }
    #[test]
    fn port_uses_saved_host_when_the_other_field_is_empty() {
        assert_eq!(
            address("44517", "", Some("192.168.18.59:5555")),
            Ok("192.168.18.59:44517".into())
        );
    }
    #[test]
    fn port_without_a_host_explains_the_required_input() {
        assert_eq!(
            address("44517", "", None),
            Err("請輸入完整 IP:埠，例如 192.168.18.59:44517.".into())
        );
    }
    #[test]
    fn zero_port_is_rejected() {
        assert_eq!(
            address("0", "192.168.18.59:44517", None),
            Err(
                "無效位址 \"192.168.18.59:0\"：請使用 IPv4 或主機名:埠號（1–65535，不支援 IPv6）"
                    .into()
            )
        );
    }
    #[test]
    fn port_above_65535_is_rejected() {
        assert_eq!(address("65536", "192.168.18.59:44517", None), Err("無效位址 \"192.168.18.59:65536\"：請使用 IPv4 或主機名:埠號（1–65535，不支援 IPv6）".into()));
    }
    #[test]
    fn full_address_preserves_its_own_host() {
        assert_eq!(
            address("192.168.18.59:44517", "old:5555", None),
            Ok("192.168.18.59:44517".into())
        );
    }
}
