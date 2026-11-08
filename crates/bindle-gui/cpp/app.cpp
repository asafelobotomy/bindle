#include "bindle-gui/cpp/app.h"

#include <QGuiApplication>
#include <QQuickWindow>
#include <QString>

namespace {
QString toQString(rust::Str s)
{
    return QString::fromUtf8(s.data(), static_cast<qsizetype>(s.size()));
}
}

void bindle_describe_application(rust::Str version)
{
    QGuiApplication::setApplicationName(QStringLiteral("bindle"));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Bindle"));
    QGuiApplication::setApplicationVersion(toQString(version));
    // The Wayland app id; matches bindle.desktop for the icon and taskbar.
    QGuiApplication::setDesktopFileName(QStringLiteral("bindle"));
}

bool bindle_has_window()
{
    return !QGuiApplication::topLevelWindows().isEmpty();
}

bool bindle_grab_window(rust::Str path)
{
    const auto file = toQString(path);
    for (QWindow *w : QGuiApplication::topLevelWindows()) {
        if (auto *quick = qobject_cast<QQuickWindow *>(w); quick && quick->isVisible())
            return quick->grabWindow().save(file);
    }
    return false;
}
