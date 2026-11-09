#include "bindle-gui/cpp/app.h"

#include <QBuffer>
#include <QGuiApplication>
#include <QIcon>
#include <QImageReader>
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

void bindle_set_window_icon(rust::Slice<const std::uint8_t> svg)
{
    const QByteArray data(reinterpret_cast<const char *>(svg.data()), static_cast<qsizetype>(svg.size()));
    QIcon icon;
    for (int size : {16, 22, 24, 32, 48, 64, 128, 256}) {
        QBuffer buffer;
        buffer.setData(data);
        QImageReader reader(&buffer, "svg");
        reader.setScaledSize(QSize(size, size));
        const QImage image = reader.read();
        if (!image.isNull())
            icon.addPixmap(QPixmap::fromImage(image));
    }
    QGuiApplication::setWindowIcon(icon.isNull() ? QIcon::fromTheme(QStringLiteral("bindle")) : icon);
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
