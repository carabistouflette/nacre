pub(super) fn implementation() -> String {
    include_str!("../../stdlib/time_clock.c.in").to_owned()
}
