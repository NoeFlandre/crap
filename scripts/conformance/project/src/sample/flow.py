"""Control flow: loops, exceptions, match, async and unreachable code."""

import asyncio
import json


def parse(text):
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        if "Expecting" in str(error):
            return None
        raise
    except (TypeError, ValueError):
        return None
    else:
        return value
    finally:
        text = None


def first_even(numbers):
    index = 0
    while True:
        if index >= len(numbers):
            break
        if numbers[index] % 2 == 0:
            return numbers[index]
        index += 1
    return None


def unreachable(flag):
    if flag:
        return 1
        print("never")  # unreachable
    else:
        raise RuntimeError("no")
    print("also never")


def dispatch(command):
    match command.split():
        case ["go", direction] if direction in {"north", "south"}:
            return f"going {direction}"
        case ["go", _]:
            return "unknown direction"
        case ["look"]:
            return "looking"
        case ["quit" | "exit"]:
            return "bye"
        case _:
            return "?"


def comprehensions(data):
    evens = [x for x in data if x % 2 == 0]
    pairs = {x: y for x in evens for y in data if x < y}
    flags = list(map(lambda v: v if v else -v, data))
    return evens, pairs, flags


async def gather(values):
    async def double(value):
        await asyncio.sleep(0)
        return value * 2

    results = []
    for coroutine in asyncio.as_completed([double(v) for v in values]):
        results.append(await coroutine)
    return sorted(results)


def debug_only():
    if __debug__:
        return "debug"
    return "optimized"


def constant_folding(x):
    if False:
        x = 1
    while 0:
        x = 2
    if 0 and x:
        x = 3
    return x


def with_blocks(path):
    with open(path) as handle:
        return handle.read()
    return ""  # reachable: a context manager may swallow exceptions


def globals_and_annotations(value):
    global CACHE
    nonlocal_name: int
    CACHE = value
    return CACHE


CACHE = None
