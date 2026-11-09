#pragma once

#include <cstdint>

#include "rust/cxx.h"

// Application name, version and Wayland app id; call after the
// QGuiApplication exists.
void bindle_describe_application(rust::Str version);
// The window icon (X11 and window lists), rendered from the SVG; falls back
// to the `bindle` theme icon when Qt can't read SVG.
void bindle_set_window_icon(rust::Slice<const std::uint8_t> svg);
// Whether QML created a window (false: Qt Quick or Kirigami missing).
bool bindle_has_window();
// Render the first visible Qt Quick window to a PNG (screenshot tests).
bool bindle_grab_window(rust::Str path);
