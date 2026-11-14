import QtQuick

// `--screenshots <dir>`: every `interval` save the window as the previous
// step's `name`, then `shoot` the next step; quit after the last. A step
// that sets `waiting` holds the next save while `busy`.
Timer {
    id: driver

    required property QtObject appBackend
    property var plan: []
    property int step: 0
    property bool waiting: false
    property bool busy: false

    signal shoot(var s)

    interval: 600
    repeat: true
    running: appBackend.screenshotDir !== ""
    onTriggered: {
        if (waiting && busy)
            return;
        waiting = false;
        if (step > 0)
            driver.appBackend.grabWindow(driver.appBackend.screenshotDir + "/" + plan[step - 1].name + ".png");
        if (step === plan.length) {
            Qt.quit();
            return;
        }
        driver.shoot(plan[step]);
        if (running)
            step += 1;
    }
}
