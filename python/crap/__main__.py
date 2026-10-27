"""Run `crap` as `python -m crap`."""

import os
import subprocess
import sys

from crap import find_crap_bin


def _run() -> None:
    crap = find_crap_bin()
    if sys.platform == "win32":
        completed = subprocess.run([crap, *sys.argv[1:]])
        sys.exit(completed.returncode)
    else:
        os.execvp(crap, [crap, *sys.argv[1:]])


if __name__ == "__main__":
    _run()
