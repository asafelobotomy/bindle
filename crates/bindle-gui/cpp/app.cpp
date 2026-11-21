#include "bindle-gui/cpp/app.h"

#include <QBuffer>
#include <QDir>
#include <QStandardPaths>
#include <QGuiApplication>
#include <QIcon>
#include <QImageReader>
#include <QLocalServer>
#include <QLocalSocket>
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

    // Outside a full desktop (Hyprland, the offscreen screenshot run) Qt may
    // search no icon folders and have no icon theme: add the XDG ones, then
    // use the first installed theme that has the names the views ask for.
    // QML hides any icon that still isn't found.
    QStringList paths = QIcon::themeSearchPaths();
    for (const QString &dir : QStandardPaths::locateAll(QStandardPaths::GenericDataLocation, QStringLiteral("icons"),
                                                        QStandardPaths::LocateDirectory)) {
        if (!paths.contains(dir))
            paths.append(dir);
    }
    QIcon::setThemeSearchPaths(paths);
    const QString current = QIcon::themeName();
    const bool none = current.isEmpty() || current == QStringLiteral("hicolor");
    for (const auto &theme : {QStringLiteral("breeze"), QStringLiteral("breeze-dark"), QStringLiteral("Papirus"),
                              QStringLiteral("Adwaita")}) {
        bool found = false;
        for (const QString &dir : QIcon::themeSearchPaths())
            found = found || QDir(dir).exists(theme + QStringLiteral("/index.theme"));
        if (!found)
            continue;
        if (none)
            QIcon::setThemeName(theme);
        else
            QIcon::setFallbackThemeName(theme);
        break;
    }
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

namespace {
void bringForward()
{
    const auto windows = QGuiApplication::allWindows();
    for (QWindow *w : windows) {
        if (!qobject_cast<QQuickWindow *>(w) || !w->isVisible())
            continue;
        if (w->windowState() & Qt::WindowMinimized)
            w->setWindowStates(w->windowStates() & ~Qt::WindowMinimized);
        w->show();
        w->raise();
        w->requestActivate();
    }
}
}

bool bindle_forward_to_running(rust::Str window)
{
    // $XDG_RUNTIME_DIR: only this user can reach it. Without one every start gets a window.
    const QString dir = qEnvironmentVariable("XDG_RUNTIME_DIR");
    if (dir.isEmpty() || !QDir::isAbsolutePath(dir) || !QDir(dir).exists())
        return false;
    const QString path = dir + QStringLiteral("/bindle-gui-") + toQString(window) + QStringLiteral(".socket");
    {
        QLocalSocket probe;
        probe.connectToServer(path);
        if (probe.waitForConnected(1000)) {
            probe.write("show\n");
            probe.waitForBytesWritten(1000);
            probe.disconnectFromServer();
            return true;
        }
    }
    // Nobody answered: a socket left by a window that crashed is stale, so take it over.
    auto *server = new QLocalServer(qApp);
    server->setSocketOptions(QLocalServer::UserAccessOption);
    if (!server->listen(path)) {
        QLocalServer::removeServer(path);
        if (!server->listen(path)) {
            qWarning("bindle-gui: can't listen on %s (%s); a second start will open a second window",
                     qPrintable(path), qPrintable(server->errorString()));
            return false;
        }
    }
    QObject::connect(server, &QLocalServer::newConnection, server, [server] {
        while (QLocalSocket *peer = server->nextPendingConnection()) {
            QObject::connect(peer, &QLocalSocket::disconnected, peer, &QObject::deleteLater);
            bringForward();
        }
    });
    QObject::connect(qApp, &QCoreApplication::aboutToQuit, server, [server] { server->close(); });
    return false;
}
