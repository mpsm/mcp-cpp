import { TestProject } from '../framework/TestProject.js';

export default async function createCmakeBasicFixture(
  project: TestProject
): Promise<void> {
  // Create a basic CMake project with configured build directories
  await project.writeFile(
    'CMakeLists.txt',
    `
cmake_minimum_required(VERSION 3.15)
project(TestProject VERSION 1.0.0)

# Add executable
add_executable(TestProject src/main.cpp)

# Optional: Add compile commands for better LSP support
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
`
  );

  await project.writeFile(
    'src/main.cpp',
    `
#include <iostream>

int main() {
    std::cout << "Hello from C++ MCP test project!" << std::endl;
    return 0;
}
`
  );

  // Configure with Debug build
  await project.runCmake({
    buildType: 'Debug',
    buildDir: 'build-debug',
  });

  // Configure with Release build
  await project.runCmake({
    buildType: 'Release',
    buildDir: 'build-release',
  });
}
