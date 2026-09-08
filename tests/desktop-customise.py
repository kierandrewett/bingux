#!/usr/bin/env python3
"""Customisation is tested in the running shell, using its actual containers."""
from pathlib import Path
import runpy

runpy.run_path(str(Path(__file__).with_name("desktop-layout-live.py")), run_name="__main__")
