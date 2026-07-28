"""Shared utility functions for the example monorepo."""


def greet(name: str) -> str:
    return f"Hello, {name}! From the Python utils library."


def add(a: int, b: int) -> int:
    return a + b


if __name__ == "__main__":
    print(greet("World"))
    print(f"1 + 2 = {add(1, 2)}")
