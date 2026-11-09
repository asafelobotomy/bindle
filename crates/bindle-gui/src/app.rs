//! Application setup and the window grab, in C++ (see `cpp/app.cpp`).

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("bindle-gui/cpp/app.h");

        fn bindle_describe_application(version: &str);
        fn bindle_set_window_icon(svg: &[u8]);
        fn bindle_has_window() -> bool;
        fn bindle_grab_window(path: &str) -> bool;
    }
}
