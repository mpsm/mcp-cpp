#pragma once

#include <string>
#include <vector>
#include <memory>

namespace TestProject {

/**
 * @brief Storage backend interface
 * 
 * Pure virtual interface for storage implementations.
 */
class IStorageBackend {
public:
    /**
     * @brief Virtual destructor for proper cleanup
     */
    virtual ~IStorageBackend() = default;

    /**
     * @brief Store a key-value pair
     * @param key The key to store
     * @param value The value to associate with the key
     * @return true if successful, false otherwise
     */
    virtual bool store(const std::string& key, const std::string& value) = 0;

    /**
     * @brief Retrieve a value by key
     * @param key The key to look up
     * @return The associated value, or empty string if not found
     */
    virtual std::string retrieve(const std::string& key) const = 0;

    /**
     * @brief Remove a key-value pair
     * @param key The key to remove
     * @return true if the key was found and removed, false otherwise
     */
    virtual bool remove(const std::string& key) = 0;

    /**
     * @brief List all stored keys
     * @return Vector of all keys in the storage
     */
    virtual std::vector<std::string> listKeys() const = 0;

    /**
     * @brief Clear all stored data
     */
    virtual void clear() = 0;

    /**
     * @brief Get the backend type name
     * @return String identifying the backend type
     */
    virtual std::string getBackendType() const = 0;

#ifdef ENABLE_DEBUG_LOGGING
    /**
     * @brief Debug method - only available when debug logging is enabled
     * @return Debug information about the storage state
     */
    virtual std::string getDebugInfo() const = 0;
#endif
};

} // namespace TestProject