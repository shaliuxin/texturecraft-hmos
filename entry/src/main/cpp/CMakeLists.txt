cmake_minimum_required(VERSION 3.5.0)
project(texturecraft_native)
set(CMAKE_CXX_STANDARD 17)
get_filename_component(TEXTURECRAFT_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../.." ABSOLUTE)
# OHOS_ARCH is the ABI hvigor is building (abiFilters in entry/build-profile.json5). This mapping
# is the only place in the C++ layer that knows about architectures: adding an ABI means adding a
# line here plus the corresponding triple in scripts/build_native.sh and scripts/verify_native.py.
if(OHOS_ARCH STREQUAL "arm64-v8a")
  set(RUST_TARGET aarch64-unknown-linux-ohos)
elseif(OHOS_ARCH STREQUAL "x86_64")
  set(RUST_TARGET x86_64-unknown-linux-ohos)
elseif(OHOS_ARCH STREQUAL "armeabi-v7a")
  set(RUST_TARGET armv7-unknown-linux-ohos)
else()
  message(FATAL_ERROR "TextureCraft does not support OHOS_ARCH=${OHOS_ARCH}")
endif()
if(NOT DEFINED TEXTURECRAFT_RUST_PROFILE)
  set(TEXTURECRAFT_RUST_PROFILE release)
endif()
# TEXTURECRAFT_RUST_TARGET_DIR points at a target directory outside the workspace (CI caches it); it
# must contain <triple>/<profile>/libtexturecraft_hmos.a for the ABI being built.
if(DEFINED TEXTURECRAFT_RUST_TARGET_DIR AND NOT TEXTURECRAFT_RUST_TARGET_DIR STREQUAL "")
  set(RUST_LIBRARY "${TEXTURECRAFT_RUST_TARGET_DIR}/${RUST_TARGET}/${TEXTURECRAFT_RUST_PROFILE}/libtexturecraft_hmos.a")
else()
  set(RUST_LIBRARY "${TEXTURECRAFT_ROOT}/native/rust/target/${RUST_TARGET}/${TEXTURECRAFT_RUST_PROFILE}/libtexturecraft_hmos.a")
endif()
if(NOT EXISTS "${RUST_LIBRARY}")
  message(FATAL_ERROR "Run scripts/build_native.sh before building HAP: ${RUST_LIBRARY}")
endif()
add_library(texturecraft SHARED bridge.cpp)
# iana_time_zone（由 chrono → texturecraft-engine / craft-update 传递引入）的 OHOS 实现调用
# OH_TimeService_GetTimeZone。已用 llvm-nm 核实两个 ABI 的 libtexturecraft_hmos.a 均含该未定义符号，
# 且 sysroot/usr/lib/{aarch64,x86_64}-linux-ohos/libtime_service_ndk.so 均导出 T 符号，
# 故与 gridcraft/vectorcraft 同源，必须链接 time_service_ndk.z（删掉会 ld.lld undefined symbol）。
# EGL/GLESv3 是 wgpu 在鸿蒙上的渲染后端（TextureCraft 的 2D/3D 视口走 egui-wgpu fragment 渲染）。
target_link_libraries(texturecraft PRIVATE "${RUST_LIBRARY}" ace_napi.z ace_ndk.z native_window native_vsync hilog_ndk.z time_service_ndk EGL GLESv3 dl m pthread)

target_link_options(texturecraft PRIVATE "-Wl,--gc-sections" "-Wl,--exclude-libs,ALL")
