# Dock application menus

Right-click an application icon in the dock to open its menu. The menu lets you
pin or unpin the application, launch a new window, activate an open window,
or close a window. Application actions come from its desktop entry.

Menus load when first opened. Hovering an icon shows its tooltip without
loading the menu. Opened menus are reused while their dock icon remains present.
Removing an icon also releases its menu.

Click outside the menu or press Escape to close it. Use the Up and Down keys to
select an action and Enter to activate it.

## Window actions

Each open window in an application menu has a ⋯ button beside its close button.
It opens a menu for that one window with only the actions valid for its current
state: Minimise or Restore, Maximise or Unmaximise, Fullscreen or Exit
fullscreen, Move to a workspace (when there is more than one), and Close.
Move and Resize are not offered yet.

Gnoblin identifies windows by its own ID, which the shell cannot read from the
dock. The shell finds the window by app ID, title, and state. If that match is
not exact, the menu refuses the action and says why: "Window is no longer open"
or "Window matches more than one Gnoblin window". A failed action reopens the
menu with the error at the top so it can be retried.
