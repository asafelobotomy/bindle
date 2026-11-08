#pragma once

#include "rust/cxx.h"

// Application name, version and Wayland app id; call after the
// QGuiApplication exists.
void bindle_describe_application(rust::Str version);
// Whether QML created a window (false: Qt Quick or Kirigami missing).
bool bindle_has_window();
// Render the first visible Qt Quick window to a PNG (screenshot tests).
bool bindle_grab_window(rust::Str path);
