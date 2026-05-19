from importlib import util
from pathlib import Path
import sys
from types import ModuleType


REPO_ROOT = Path(__file__).resolve().parents[2]


def load_script_module(relative_path: str, module_name: str) -> ModuleType:
    spec = util.spec_from_file_location(module_name, REPO_ROOT / relative_path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load module from {relative_path}")

    module = util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module
