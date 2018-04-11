#!/usr/bin/env python3
# Prints differences in redirect behaviour between two shells.
import os
import subprocess
import sys
from tempfile import TemporaryDirectory


def execute(shell, src):
    with TemporaryDirectory() as tmpdir:
        p = subprocess.run(
            [shell],
            input=f"cd {tmpdir}; {src}".encode("utf-8"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        )

        fs = {}
        for (dirpath, _, filenames) in os.walk(tmpdir):
            for filename in filenames:
                path = os.path.join(dirpath, filename)
                with open(path) as f:
                    key = path[len(tmpdir) + 1:]
                    fs[key] = f.read()

        return p.stdout.decode("utf-8"), p.stderr.decode("utf-8"), fs


def main(sh1, sh2):
    code = 0
    cmds = (
        "echo a >&2",
        "echo b 1>&2",
        "echo c 2>&1",
        "echo d >file",
        "echo e 1>file",
        "echo f >>file; echo f 1>>file",
        "echo g >>file; echo g >file",
        "echo h 2>file",
        "echo i >file; cat <file",
        "echo j >file; cat <file >&2",
        "echo k >foo >bar >baz",
        "echo l >file | cat",
    )

    sh1_prog = os.path.basename(sh1)
    sh2_prog = os.path.basename(sh2)

    for cmd in cmds:
        sh1_state = execute(sh1, cmd)
        sh2_state = execute(sh2, cmd)

        if sh1_state != sh2_state:
            print(f"CMD: {cmd}")
            print("%4s:" % sh1_prog, sh1_state)
            print("%4s:" % sh2_prog, sh2_state)
            code = 1

    return code


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <sh1> <sh2>", file=sys.stderr)
        exit(1)

    sys.exit(main(sys.argv[1], sys.argv[2]))
