---
name: python-style
description: >
    Python coding conventions: uv commands (sync/add/run), pytest, ruff, ty, pytest-mock with autospec, src/ layout, dataclass Value Objects (frozen=True, slots=True), naming, imports, docstrings. Load whenever writing, reviewing, refactoring, linting, typechecking, testing, or PLANNING Python code — even in plan mode, before any code exists. Also load for Python test environment discussions such as pytest, uv run --with pytest, ruff, or ty. Do NOT use for project bootstrap (`uv init`, flake.nix, initial `pyproject.toml` scaffolding) — that is `nix-dev-init`'s job. Without this skill, Claude or Codex falls back to pip, unittest.mock, and wrong class patterns this user rejects.
---

# Python Coding Style Guidelines

## Scope

This skill governs **code written inside an already-bootstrapped Python project** — class design, test authoring, refactoring, code review, naming, imports, docstrings.

Project bootstrap (flake.nix, direnv, `uv init`, initial `pyproject.toml` merge) belongs to the `nix-dev-init` skill. If the project is not yet bootstrapped, defer to `nix-dev-init` first and return here once `direnv allow` succeeds and the shell has `uv` on PATH.

### Why the split

Keeping bootstrap out of this skill has two benefits: (1) when Claude is triggered to write or refactor Python code in an existing project, it does not read a long bootstrap procedure it does not need; (2) the nix `flake.nix` → `direnv` → `uv init` → `pyproject.toml` ordering is an invariant owned by `nix-dev-init` — duplicating a shortcut here would let it drift.

## Package Management

- Use only `uv` for package management, don't use `pip`
- Install dependencies using `uv sync`
- Run tools using `uv run {tool}`
- Baseline tooling dependencies belong in template-defined dependency groups, not ad hoc commands
- Add project-specific dependencies using `uv add {package}`
- Add project-specific tool dependencies using `uv add --group <group> {package}`
- Upgrade pinned packages using `uv lock --upgrade-package {package}`
- Prohibited: `uv pip install`, `uv add --dev`, `@latest`

## Directory Structure

- Store production code including entry points in `./src` directory
- Store test code in `./tests` directory
- For an application or internal project, prefer flat module placement like `src/main.py`
- Use `src/<package_name>/` only when the project is explicitly a distributable package or library with a real package namespace
- If a tool or template generates `src/<package_name>/` by default, do not keep it unless the project actually needs package semantics

## File Standards

- Keep code within 119 characters per line (URLs may exceed this limit)
- Always include type hints
- Use ty for static type checking

## Static Type Checking

- ty should already be provided by the project's `typecheck` dependency group
- Run type checks using `uv run ty check`
- Check specific paths using `uv run ty check src tests`
- Use `uv run ty server` only for editor or language-server integration
- Do not introduce another type checker unless the existing project is already standardized on it

## Testing

- Use only `pytest` as test framework, do not use `unittest`
- Run tests using `uv run --frozen pytest`
- Use `anyio` for async tests, do not use `asyncio`
- Use `pytest-mock` for mocking (`mocker` fixture), do not use `unittest.mock` directly
- `pytest`, `anyio`, and `pytest-mock` belong in the template's test dependency group
- Test coverage should include edge cases and errors
- Always add tests for new features
- Add unit tests for bug fixes

### Test Structure

- **Function-based by default**; class only for namespace grouping or `setup_method`/`teardown_method`
- **`@pytest.fixture`**: shared setup across 2+ tests, external resources, or teardown via `yield`; put in `conftest.py` when shared across files
- **Helper function**: prefer over fixture when arguments need to be passed or setup is lightweight
- **`@pytest.mark.parametrize`**: same logic with different inputs
- **Fixture scope**: default `function`; use `module`/`session` only when setup is expensive
- **Mocking**: use `mocker.patch("mod.func", autospec=True)` — enforces real signature, catches wrong-argument bugs silently missed by plain mocks

Async test pattern:

```python
# conftest.py
import pytest

@pytest.fixture
def anyio_backend() -> str:
    return "asyncio"

# tests/test_xxx.py
import pytest

@pytest.mark.anyio
async def test_something() -> None:
    result = await some_async_func()
    assert result == expected
```

## Syntax Rules

### Classes

- In Python modules, prefer `Enum -> model/class -> function` ordering when those concepts coexist
- Use `__slots__` to restrict variables when not using `dataclasses` or `pydantic`

```python
class Foo:
    __slots__ = ("name", "age")

    def __init__(self, name: str, age: int) -> None:
        self.name = name
        self.age = age
```

- Value Objects:
    - **When `pydantic` is a project dependency**: use `pydantic.BaseModel` (see [Pydantic](#pydantic) section)
    - **When `pydantic` is not a dependency**: use `dataclasses.dataclass(frozen=True, slots=True)` — `slots=True` (3.10+) auto-generates `__slots__`, no need to write it manually
    - When difficult to determine, defer judgment to user
    - Use `__post_init__` to enforce invariants:

```python
@dataclass(frozen=True, slots=True)
class Age:
    value: int

    def __post_init__(self) -> None:
        if self.value < 0:
            raise ValueError(f"Age must be non-negative, got {self.value}")
```

- Basically don't use `@staticmethod` — needing it indicates a design error
- **Use classmethods as alternative constructors for pure construction** (no I/O). `from_X`, `parse_X`, `create_X`, `make_X` → classmethod. **If construction involves I/O (network, file system), separate into a module-level function** to keep the Value Object pure and testable.

```python
from typing import Self

@dataclass(frozen=True, slots=True)
class SWEInstance:
    instance_id: str
    repo: str

    @classmethod
    def from_dict(cls, data: dict[str, str]) -> Self:   # pure → classmethod
        return cls(instance_id=data["id"], repo=data["repo"])


def load_instance(instance_id: str) -> SWEInstance:     # I/O → module-level function
    row = fetch_from_dataset(instance_id)
    return SWEInstance.from_dict(row)


# WRONG — module-level factory for pure construction:
def swe_instance_from_dict(data: dict) -> SWEInstance: ...  # move inside as classmethod
```

### Pydantic

Frozen model config: `pydantic.ConfigDict(extra="forbid", frozen=True, strict=True, validate_default=True)`

When multiple frozen models exist, extract a shared base class to `base.py`:

```python
import pydantic


class FrozenModel(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid", frozen=True, strict=True, validate_default=True)
```

### Enums

- Use `Enum`/`StrEnum` for fixed choices instead of raw string literals or dict maps
- Convert user input strings to Enum early, then map through Enum values
- Prefer `StrEnum` when values are serialized or user-facing

### Functions

- Keep functions focused and small
- Use Pythonic syntax (comprehensions, `with` statements, etc.)
- Don't use `global`, `nonlocal` (not explicit enough)
- Use built-in generics (e.g., `tuple`, `list`, `dict`) instead of `typing.Tuple`, `typing.List`, `typing.Dict`

### Strings

#### Quote Usage

- String contains `'` → use `"`
- String contains `"` → use `'`
- f-strings (variable substitution) → use `"`
- `raise` statements → use `"` (normal sentences use `'`)

#### Operators

- Separate operators and operands by one space
- When using 2+ operators, omit spaces around `*`, `/`, `//`, `%`, `**` (higher precedence)

### Logging

Write in dictionary format. Add extensive logging at critical system points where failures would be hard to diagnose (CSV file references, before/after raise statements, etc.).

```python
logger.info({
    "action": "save",
    "csv_file": self.csv_file,
    "status": "run"
})
```

## Naming Conventions

- Constants: `SCREAMING_SNAKE_CASE` — define semantically meaningful string literals as module-level constants (two blank lines after imports) rather than embedding them inline
- Variables / functions / files: `snake_case`
    - Getters: name of the output variable
- Classes: `UpperCamelCase`
- Iterator arguments:
    - Loop body ≤ 2 lines: single character (`x`, `i`)
    - Loop body ≥ 3 lines: descriptive name

### File Naming

- Name files after the **domain/action** they represent, not the role suffix
    - Prefer `retrieval.py` over `retriever.py`, `prompt.py` over `prompt_builder.py`
    - `-er`/`-or` suffixes belong on **class names** (e.g. `class Retriever`), not file names
- Avoid `utils.py` / `helpers.py` — name by what the module actually does (e.g. `model.py`, `inference.py`)

## Whitespace & Layout

### Two blank lines before/after

- Import statements
- Global variable definitions
- Object (class/function) definitions

### One blank line between

- Function/method docstring sections: summary `"""`, detail, `Args`, `Returns/Yields`, `Raises`
- Import groups (standard / third-party / local / personal)
- Instance methods

### Imports

- Order import groups as: standard library → third-party → local
- Use exactly one blank line between these groups
- Avoid `from ... import ...` unless the imported name is self-explanatory (e.g., `Enum`, `Path`), or the module path is so long that `module.name` at call sites becomes unwieldy (e.g., `from swebench.inference.make_datasets.utils import extract_diff`)
- Avoid `as` aliases unless required for clarity

### Indentation

- When handling multiple objects in parallel, align indentation with the previous element

### Line Breaks

- One element per line for lists/dicts with 3+ items or long expressions; trailing comma on last element
- Don't sacrifice readability for brevity

```python
# Good
sections = [
    problem_statement,
    *_file_sections(files),
    "Please output a unified diff patch.",
]

# Bad
sections = [problem_statement, *_file_sections(files), "Please output a unified diff patch."]
```

## Comments & Docstrings

- Write comments on their own line above the relevant code
- Always add docstrings to public APIs
- Comment non-obvious choices (algorithm params, fallback behavior, encoding handling)

```python
# File level
"""Explanation of file functionality"""

# Class & method level
class MyClass:
    """Class functionality explanation"""

    def method(self):
        """Method functionality explanation"""

# Function level
def function_name(arg1, arg2):
    """Function functionality summary.

    (Detailed function functionality.)

    Args:
        arg1 (type): Argument description
        arg2 (type): Argument description

    Returns/Yields:
        type: Return value description

    Raises:
        ErrorType: Error description

    (see details at: URL)
    """
```
