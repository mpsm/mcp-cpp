#pragma once

#include "IStorageBackend.hpp"
#include <string>
#include <vector>
#include <memory>

// Conditional include based on compile-time definitions
#ifdef USE_MEMORY_STORAGE
#include "MemoryStorage.hpp"
#else
#include "FileStorage.hpp"
#endif

namespace TestProject {

/**
 * @brief Storage backend factory
 * 
 * This demonstrates compile-time polymorphism using preprocessor macros.
 * The actual implementation is selected at compile time based on CMake options.
 */
class StorageBackend {
public:
    /**
     * @brief Create a storage backend instance
     * @return Unique pointer to the selected storage implementation
     */
    static std::unique_ptr<IStorageBackend> create();
};

// Compile-time type alias for the selected backend
#ifdef USE_MEMORY_STORAGE
using SelectedBackend = MemoryStorage;
#else
using SelectedBackend = FileStorage;
#endif

} // namespace TestProject