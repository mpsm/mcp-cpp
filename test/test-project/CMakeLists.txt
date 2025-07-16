cmake_minimum_required(VERSION 3.16)
project(TestProject VERSION 1.0.0 LANGUAGES CXX)

# Set C++ standard (updated to C++20 for concepts support)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Enable compile_commands.json generation
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# CMake options for conditional compilation
option(USE_MEMORY_STORAGE "Use in-memory storage backend" ON)
option(ENABLE_DEBUG_LOGGING "Enable debug logging" OFF)

# Set compiler flags
set(CMAKE_CXX_FLAGS_DEBUG "-g -O0 -Wall -Wextra -pedantic")
set(CMAKE_CXX_FLAGS_RELEASE "-O3 -DNDEBUG")

# Create library target
add_library(TestLib STATIC)

# Base source files
target_sources(TestLib PRIVATE
    src/Math.cpp
    src/StringUtils.cpp
    src/Container.cpp
    src/EnumOperators.cpp
)

# Conditionally add storage backend implementations
if(USE_MEMORY_STORAGE)
    target_sources(TestLib PRIVATE src/MemoryStorage.cpp)
else()
    target_sources(TestLib PRIVATE src/FileStorage.cpp)
endif()

target_include_directories(TestLib PUBLIC include)

# Configure compile-time definitions based on options
if(USE_MEMORY_STORAGE)
    target_compile_definitions(TestLib PUBLIC USE_MEMORY_STORAGE)
else()
    target_compile_definitions(TestLib PUBLIC USE_FILE_STORAGE)
endif()

if(ENABLE_DEBUG_LOGGING)
    target_compile_definitions(TestLib PUBLIC ENABLE_DEBUG_LOGGING)
endif()

# Create executable
add_executable(${PROJECT_NAME})

target_sources(${PROJECT_NAME} PRIVATE
    src/main.cpp
)

target_link_libraries(${PROJECT_NAME} PRIVATE TestLib)