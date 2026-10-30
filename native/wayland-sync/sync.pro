QT += quick gui-private qml waylandclient
CONFIG += plugin c++17 link_pkgconfig
TEMPLATE = lib
TARGET = binguxwaylandsync
PKGCONFIG += wayland-client
SOURCES = plugin.cpp launch-activation.cpp
HEADERS = launch-activation.h

WAYLANDCLIENTSOURCES += $$PWD/xdg-activation-v1.xml
load(wayland-scanner)
