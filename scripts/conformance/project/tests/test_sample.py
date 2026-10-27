import asyncio

import pytest

from sample import flow, shapes


def test_shapes():
    circle = shapes.Circle(1.0)
    assert circle.describe() == "Circle: 3.14"
    square = shapes.Polygon([2, 2, 2, 2])
    assert square.classify() == "square"
    assert square.perimeter() == 8
    with pytest.raises(ValueError):
        shapes.Polygon([-1]).perimeter()
    assert shapes.fibonacci(10) == 55


def test_flow():
    assert flow.parse("[1]") == [1]
    assert flow.parse("{") is None
    assert flow.first_even([1, 3, 4]) == 4
    assert flow.first_even([1]) is None
    assert flow.unreachable(True) == 1
    assert flow.dispatch("go north") == "going north"
    assert flow.dispatch("quit") == "bye"
    assert flow.comprehensions([1, 2, 3, 4])[0] == [2, 4]
    assert asyncio.run(flow.gather([1, 2])) == [2, 4]
    assert flow.debug_only() == "debug"
    assert flow.constant_folding(5) == 5
    assert flow.globals_and_annotations(3) == 3
