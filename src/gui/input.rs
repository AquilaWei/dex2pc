pub(super) fn host(saved: Option<&str>) -> Option<&str> {
    saved.and_then(|value| value.split_once(':').map(|(host, _)| host))
}
