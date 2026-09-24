QT += gui gui-private qml waylandclient
CONFIG += plugin c++17 link_pkgconfig
TEMPLATE = lib
TARGET = binguxwaylandsync
PKGCONFIG += wayland-client
SOURCES = plugin.cpp
