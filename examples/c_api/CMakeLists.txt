cmake_minimum_required(VERSION 3.10)
project(sea_codec_c_example)

set(CMAKE_C_STANDARD 99)

# Include directories
include_directories(../../include)

# Find the library
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release")
else()
    set(LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/debug")
endif()

message(STATUS "Looking for library in ${LIB_DIR}")

find_library(SEA_LIB NAMES sea_codec libsea_codec sea_codec.dll PATHS ${LIB_DIR} NO_DEFAULT_PATH)

if(NOT SEA_LIB)
    message(WARNING "sea-codec library not found in ${LIB_DIR}. Please build the rust project first (cargo build).")
endif()

add_executable(c_example main.c)

if(SEA_LIB)
    target_link_libraries(c_example ${SEA_LIB})
    if(UNIX) 
        target_link_libraries(c_example m pthread dl) 
    endif()
endif()
