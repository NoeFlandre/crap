"""Classes, properties, decorators and nested functions."""

from __future__ import annotations

import functools
import math
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterable


def memoize(function):
    cache = {}

    @functools.wraps(function)
    def wrapper(*args):
        if args not in cache:
            cache[args] = function(*args)
        return cache[args]

    return wrapper


@dataclass
class Shape:
    name: str

    @property
    def label(self) -> str:
        """The label."""
        return self.name.title()

    def area(self) -> float:
        raise NotImplementedError

    def describe(
        self,
        precision: int = 2,
    ) -> str:
        area = round(
            self.area(),
            precision,
        )
        return (
            f"{self.label}: "
            f"{area}"
        )


class Circle(Shape):
    def __init__(self, radius: float) -> None:
        super().__init__("circle")
        self.radius = radius

    def area(self) -> float:
        return math.pi * self.radius**2


class Polygon(Shape):
    def __init__(self, sides: Iterable[float]) -> None:
        super().__init__("polygon")
        self.sides = list(sides)

    def area(self) -> float:  # pragma: no cover
        raise ValueError("unknown")

    def perimeter(self) -> float:
        total = 0.0
        for side in self.sides:
            if side < 0:
                raise ValueError("negative side")
            total += side
        else:
            return total

    def classify(self) -> str:
        count = len(self.sides)
        if count == 3:
            return "triangle"
        elif count == 4:
            sides = set(self.sides)
            return "square" if len(sides) == 1 else "quadrilateral"
        elif count > 4 and all(s == self.sides[0] for s in self.sides):
            return "regular"
        return "polygon"


@memoize
def fibonacci(n: int) -> int:
    return n if n < 2 else fibonacci(n - 1) + fibonacci(n - 2)
