project('test-meson-project', 'cpp',
  version : '1.0.0',
  default_options : ['warning_level=3',
                     'cpp_std=c++17'])

# Include directories
inc = include_directories('include')

# Source files
sources = files(
  'src/main.cpp',
  'src/math.cpp',
  'src/utils.cpp'
)

# Create executable
executable('test_meson_app',
  sources,
  include_directories : inc,
  install : true)

# Create a static library as well
lib_sources = files(
  'src/math.cpp',
  'src/utils.cpp'
)

static_library('test_meson_lib',
  lib_sources,
  include_directories : inc,
  install : true)