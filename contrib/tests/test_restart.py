"""Exercise the real launcher's restart/exit path without touching Kobo hardware."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest


LAUNCHER = Path(__file__).resolve().parents[1] / "plato.sh"


class RestartTests(unittest.TestCase):
    def run_launcher(self, actions, stale=False, swap=False, usb_status=0):
        with tempfile.TemporaryDirectory(prefix="plato-restart-") as directory:
            root = Path(directory)
            old, new = root / "old", root / "new"
            old.mkdir()
            new.mkdir()
            workdir = root / "current"
            workdir.symlink_to(old, target_is_directory=True)
            probe = "#!" + os.sys.executable + "\n" + '''
import json, os
from pathlib import Path
root = Path(os.environ["TEST_ROOT"])
count_file = root / "count"
count = int(count_file.read_text()) if count_file.exists() else 0
count_file.write_text(str(count + 1))
with (root / "trace").open("a") as trace:
    trace.write("plato:" + Path.cwd().name + "\\n")
action = json.loads(os.environ["TEST_ACTIONS"])[count]
if "restart" in action:
    (root / "restart").touch()
if "reboot" in action:
    (root / "reboot").touch()
if "poweroff" in action:
    (root / "power_off").touch()
if action == "share":
    (root / "share").touch()
    (root / "reader_pid").write_text(str(os.getpid()))
if os.environ.get("PLATO_USB_RESULT"):
    with (root / "trace").open("a") as trace:
        trace.write("usb_result:" + os.environ["PLATO_USB_RESULT"] + "\\n")
if count == 0 and os.environ.get("TEST_SWAP"):
    replacement = root / "next"
    replacement.symlink_to(root / "new", target_is_directory=True)
    replacement.replace(root / "current")
raise SystemExit(1 if action == "crash" else 0)
'''
            for app in (old, new):
                (app / "plato").write_text(probe)
                (app / "plato").chmod(0o755)
                (app / "nickel.sh").write_text('#!/bin/sh\nprintf "nickel\\n" >> "$TEST_ROOT/trace"\n')
                (app / "nickel.sh").chmod(0o755)
                (app / "scripts").mkdir()
                for name in ("usb-enable.sh", "usb-disable.sh"):
                    (app / "scripts" / name).touch()
                usb = app / "scripts" / "usb-session.sh"
                usb.write_text("#!" + os.sys.executable + "\n" + '''
import os
from pathlib import Path
root = Path(os.environ["TEST_ROOT"])
assert Path.cwd() not in (root / "old", root / "new")
try:
    os.kill(int((root / "reader_pid").read_text()), 0)
except ProcessLookupError:
    pass
else:
    raise RuntimeError("Plato is still running during USB export")
with (root / "trace").open("a") as trace:
    trace.write("usb\\n")
status = int(os.environ["TEST_USB_STATUS"])
if status == 0:
    replacement = root / "next"
    replacement.symlink_to(root / "new", target_is_directory=True)
    replacement.replace(root / "current")
raise SystemExit(status)
''')
                usb.chmod(0o755)
            commands = root / "commands"
            commands.mkdir()
            for command in ("reboot", "poweroff"):
                path = commands / command
                path.write_text(f'#!/bin/sh\nprintf "{command}\\n" >> "$TEST_ROOT/trace"\n')
                path.chmod(0o755)
            rm = commands / "rm"
            rm.write_text('#!/bin/sh\n[ "$(pwd)" = / ] || exit 1\nexec /bin/rm "$@"\n')
            rm.chmod(0o755)
            if stale:
                (root / "restart").touch()
            # The preamble configures real hardware. Run the unmodified exit
            # path, redirecting the marker paths into the fixture.
            supervisor = LAUNCHER.read_text().split("exec /bin/sh -c ", 1)[1]
            script = shlex.split(supervisor)[0]
            for marker, filename in (("plato_restart", "restart"), ("reboot", "reboot"),
                                     ("power_off", "power_off"), ("plato_usb_share", "share")):
                script = script.replace("/tmp/" + marker, shlex.quote(str(root / filename)))
            # Wait for the normal asynchronous Nickel launch before inspecting.
            script += "\nwait\n"
            env = dict(os.environ, WORKDIR=str(workdir), TEST_ROOT=str(root),
                       TEST_ACTIONS=json.dumps(actions), TEST_USB_STATUS=str(usb_status), ORIG_BPP="",
                       PATH=str(commands) + os.pathsep + os.environ["PATH"])
            if swap:
                env["TEST_SWAP"] = "1"
            launch = 'cd / && exec /bin/sh -c ' + shlex.quote(script) + ' plato-supervisor "$WORKDIR" "$ORIG_BPP"'
            subprocess.run(["sh", "-c", launch], cwd=old, env=env, check=True, timeout=10)
            return (root / "trace").read_text().splitlines()

    def test_restart_then_quit(self):
        self.assertEqual(self.run_launcher(["restart", "restart", "quit"]),
                         ["plato:old", "plato:old", "plato:old", "nickel"])

    def test_crash_does_not_restart(self):
        self.assertEqual(self.run_launcher(["crash"]), ["plato:old", "nickel"])

    def test_stale_request_does_not_restart(self):
        self.assertEqual(self.run_launcher(["quit"], stale=True), ["plato:old", "nickel"])

    def test_reboot_takes_priority(self):
        self.assertEqual(self.run_launcher(["restart+reboot"]), ["plato:old", "reboot"])

    def test_poweroff_takes_priority(self):
        self.assertEqual(self.run_launcher(["restart+poweroff"]), ["plato:old", "poweroff"])

    def test_restart_reenters_current_mount(self):
        self.assertEqual(self.run_launcher(["restart", "quit"], swap=True),
                         ["plato:old", "plato:new", "nickel"])

    def test_usb_handoff_relaunches_from_remounted_storage(self):
        self.assertEqual(self.run_launcher(["share", "quit"]),
                         ["plato:old", "usb", "plato:new", "usb_result:done", "nickel"])

    def test_usb_setup_failure_returns_to_plato(self):
        self.assertEqual(self.run_launcher(["share", "quit"], usb_status=1),
                         ["plato:old", "usb", "plato:old", "usb_result:failed", "nickel"])


if __name__ == "__main__":
    unittest.main()
