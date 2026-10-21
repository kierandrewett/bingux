cmake_minimum_required(VERSION 3.20)
project(bingux-shell VERSION 0.1.0 LANGUAGES CXX C)

# bingux-shell is Quickshell with Bingux's runtime patches, built into one
# binary whose entry point knows the Bingux shells (desktop, lock, greeter,
# popouts, wallpaper) and the packed QML archive. Quickshell is a pinned
# submodule; prepare-quickshell.py copies it into the build tree and applies
# packages/bingux-quickshell/*.patch so the submodule stays clean.

set(BINGUX_LIBDIR "lib" CACHE STRING "Library directory name under the install prefix (lib or lib64)")
set(BINGUX_SHELL_ARCHIVE_NAME "bingux-shell.sxa" CACHE STRING "File name of the packed shell archive")
set(BINGUX_SHELL_VERSION "${PROJECT_VERSION}" CACHE STRING "Version reported by bingux-shell --version")

set(QS_SUBMODULE "${CMAKE_CURRENT_SOURCE_DIR}/quickshell")
set(QS_PATCHES "${CMAKE_CURRENT_SOURCE_DIR}/../bingux-quickshell")
set(QS_SOURCE "${CMAKE_BINARY_DIR}/quickshell-src")
execute_process(
    COMMAND "${CMAKE_COMMAND}" -E env python3 "${CMAKE_CURRENT_SOURCE_DIR}/prepare-quickshell.py"
            "${QS_SUBMODULE}" "${QS_PATCHES}" "${QS_SOURCE}"
    RESULT_VARIABLE prepare_result
)
if(NOT prepare_result EQUAL 0)
    message(FATAL_ERROR "could not prepare the patched Quickshell source")
endif()
# Reconfigure when the submodule pin or a patch changes.
file(GLOB qs_patch_files "${QS_PATCHES}/*.patch")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    "${QS_SUBMODULE}/.git" "${QS_SOURCE}.stamp" ${qs_patch_files})

# Quickshell build options. Bingux runs on Gnoblin (and any layer-shell
# compositor for its general UI): the Hyprland and i3 modules are off, which
# also keeps the build working on Qt 6.12. Crash handling is systemd's job;
# the shell runs with core dumps disabled.
set(DISTRIBUTOR "Bingux (bingux-shell)" CACHE STRING "" FORCE)
set(DISTRIBUTOR_DEBUGINFO_AVAILABLE OFF CACHE BOOL "" FORCE)
set(CRASH_REPORTER OFF CACHE BOOL "" FORCE)
set(HYPRLAND OFF CACHE BOOL "" FORCE)
set(I3 OFF CACHE BOOL "" FORCE)
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(INSTALL_QML_PREFIX "${BINGUX_LIBDIR}/bingux/qml" CACHE STRING "" FORCE)
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE RelWithDebInfo CACHE STRING "" FORCE)
endif()

add_subdirectory("${QS_SOURCE}" quickshell EXCLUDE_FROM_ALL)

# Quickshell's executable target is `quickshell` with its own main.cpp. Swap
# that file for the Bingux entry point and rename the output.
get_target_property(qs_sources quickshell SOURCES)
list(FILTER qs_sources EXCLUDE REGEX "(^|/)main\\.cpp$")
set_target_properties(quickshell PROPERTIES SOURCES "${qs_sources}" OUTPUT_NAME bingux-shell)
target_sources(quickshell PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/archive.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/archive.hpp"
)
target_include_directories(quickshell PRIVATE "${QS_SOURCE}/src")
find_package(PkgConfig REQUIRED)
pkg_check_modules(LIBARCHIVE REQUIRED IMPORTED_TARGET GLOBAL libarchive)
target_link_libraries(quickshell PRIVATE PkgConfig::LIBARCHIVE)
target_compile_definitions(quickshell PRIVATE
    BINGUX_LIBDIR="${BINGUX_LIBDIR}"
    BINGUX_SHELL_ARCHIVE_NAME="${BINGUX_SHELL_ARCHIVE_NAME}"
    BINGUX_SHELL_VERSION="${BINGUX_SHELL_VERSION}"
)
# The binary is the only thing this project produces; the install goes
# through scripts/install-shell.py like the rest of Bingux.
add_custom_target(bingux-shell ALL DEPENDS quickshell)
