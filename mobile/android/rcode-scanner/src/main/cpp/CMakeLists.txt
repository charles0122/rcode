cmake_minimum_required(VERSION 3.22.1)
project(rcode_jni LANGUAGES CXX)

if(NOT DEFINED RCODE_LIBRARY_DIR)
    message(FATAL_ERROR "RCODE_LIBRARY_DIR must point at staged Rust Android libraries")
endif()

add_library(rcode SHARED IMPORTED)
set_target_properties(rcode PROPERTIES
    IMPORTED_LOCATION "${RCODE_LIBRARY_DIR}/${ANDROID_ABI}/librcode.so"
)

add_library(rcode_jni SHARED rcode_jni.cpp)
target_include_directories(rcode_jni PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../ffi/include")
target_compile_features(rcode_jni PRIVATE cxx_std_17)
find_library(android_log log)
target_link_libraries(rcode_jni PRIVATE rcode ${android_log})
