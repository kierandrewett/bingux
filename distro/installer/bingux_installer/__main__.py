"""Allow `python3 -m bingux_installer` to run the client CLI."""

from .cli import main


if __name__ == "__main__":
    raise SystemExit(main())
