"""Drive `bindle install` (install.sh with no arguments) like a person at the
console: run it in a pseudo-terminal, wait for each page, press keys.

    python drive-install.py <install.sh> <username> <password>

Runs on the live ISO (guest-install.sh). Takes the defaults everywhere (the
recommended desktop and profile, the preferred disk erased, no encryption),
names the computer bindle-vm, presses q on the last screen instead of
restarting, and exits 0 only if Bindle says the system is ready. Progress
goes to stdout (the serial console) as BINDLE-VM lines.
"""

import fcntl
import os
import pty
import re
import select
import struct
import sys
import termios
import time

ANSI = re.compile(rb"\x1b(\[[0-9;?]*[ -/]*[@-~]|\][^\x07]*\x07|[()][0-9A-Za-z]|[=>])")
KEYS = {"enter": b"\r", "bs": b"\x7f", "q": b"q"}


def log(msg):
    print(f"BINDLE-VM: wizard: {msg}", flush=True)


class Screen:
    """The child's output with escape codes and all whitespace removed.
    ratatui redraws only changed cells and skips with cursor moves, so text
    is matched without its spaces."""

    def __init__(self, fd):
        self.fd, self.text, self.mark = fd, "", 0
        self.raw = b""  # escape codes as spaces: for showing, not matching

    def pump(self, timeout):
        r, _, _ = select.select([self.fd], [], [], timeout)
        if not r:
            return True
        try:
            data = os.read(self.fd, 65536)
        except OSError:
            return False  # the child ended
        if not data:
            return False
        self.raw = (self.raw + ANSI.sub(b" ", data))[-20000:]
        flat = re.sub(rb"\s+", b"", ANSI.sub(b"", data))
        self.text += flat.decode("utf-8", "replace")
        return True

    def wait(self, needles, timeout, what):
        """Wait for one of `needles` in output newer than the last wait;
        returns the one seen."""
        if isinstance(needles, str):
            needles = [needles]
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            for needle in needles:
                want = re.sub(r"\s+", "", needle)
                if want in self.text[self.mark:]:
                    self.mark = self.text.index(want, self.mark) + len(want)
                    log(f"on {what}")
                    return needle
            if not self.pump(1.0):
                break
        log(f"FAILED waiting for {what} ({needle!r})")
        self.show()
        sys.exit(1)

    def show(self):
        """The last output, readable enough to see what the screen said."""
        text = re.sub(r" {2,}", "  ", self.raw.decode("utf-8", "replace"))
        for line in text[-3000:].splitlines():
            if line.strip():
                print(f"BINDLE-VM: screen: {line.strip()[:200]}", flush=True)

    def send(self, *keys, pause=0.4):
        for k in keys:
            os.write(self.fd, KEYS.get(k, k.encode() if isinstance(k, str) else k))
            end = time.monotonic() + pause
            while time.monotonic() < end and self.pump(0.1):
                pass


def main():
    install_sh, user, password = sys.argv[1:4]
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "linux"
        os.execvp("bash", ["bash", install_sh])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    s = Screen(fd)

    s.wait("Bindle install", 300, "the wizard")
    s.wait("Network", 30, "Network")
    s.send("enter")  # online through QEMU's network: on to Desktop
    s.wait("What gets installed", 30, "Desktop")
    s.send("enter")
    s.wait("Recommended for this hardware", 30, "Hardware")
    s.send("enter")
    s.wait("will be erased", 30, "Disk")
    s.send("enter")
    s.wait("Computer name", 30, "You")
    s.send("enter")  # keyboard: us, as the console has
    s.send(*["bs"] * 20, pause=0.05)
    s.send("bindle-vm", "enter", user, "enter", password, "enter", password, "enter")
    s.send("enter", "enter")  # timezone and language as they are
    s.wait("will be installed", 30, "Apps")
    s.send("enter")
    s.wait("nothing changes until", 30, "Review")
    s.send("i")
    s.send("y")
    log("install confirmed")

    # The progress screen: archinstall, then Bindle's setup (can take long).
    # Ready: "Enter restart now · q stay"; otherwise "q leave ...".
    seen = s.wait(["restart now", "q leave"], 4 * 3600, "the last screen")
    log(f"last screen: {seen}")
    if seen != "restart now":
        s.show()
    s.send("q", pause=2)
    while s.pump(1.0):
        pass
    _, status = os.waitpid(pid, 0)
    code = os.waitstatus_to_exitcode(status)
    ready = "Yoursystemisready" in s.text
    log(f"bindle install exited {code}, ready={ready}")
    sys.exit(0 if code == 0 and ready else 1)


if __name__ == "__main__":
    main()
