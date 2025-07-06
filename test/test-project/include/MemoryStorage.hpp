#pragma once

#include "IStorageBackend.hpp"
#include <string>
#include <vector>
#include <unordered_map>
#include <memory>

namespace TestProject {

/**
 * @brief In-memory storage implementation
 * 
 * This implementation stores data in memory using an unordered_map.
 * Fast access but data is lost when the program terminates.
 */
class MemoryStorage : public IStorageBackend {
public:
    /**
     * @brief Construct a new MemoryStorage instance
     */
    MemoryStorage();

    /**
     * @brief Destructor
     */
    ~MemoryStorage() override = default;

    /**
     * @brief Store a key-value pair in memory
     * @param key The key to store
     * @param value The value to associate with the key
     * @return true if successful (always true for memory storage)
     */
    bool store(const std::string& key, const std::string& value) override;

    /**
     * @brief Retrieve a value by key from memory
     * @param key The key to look up
     * @return The associated value, or empty string if not found
     */
    std::string retrieve(const std::string& key) const override;

    /**
     * @brief Remove a key-value pair from memory
     * @param key The key to remove
     * @return true if the key was found and removed, false otherwise
     */
    bool remove(const std::string& key) override;

    /**
     * @brief List all stored keys in memory
     * @return Vector of all keys in the storage
     */
    std::vector<std::string> listKeys() const override;

    /**
     * @brief Clear all stored data from memory
     */
    void clear() override;

    /**
     * @brief Get the backend type name
     * @return String identifying this as memory storage
     */
    std::string getBackendType() const override;

#ifdef ENABLE_DEBUG_LOGGING
    /**
     * @brief Get debug information about memory storage state
     * @return Debug information string
     */
    std::string getDebugInfo() const override;
#endif

    /**
     * @brief Get the current number of stored items
     * @return Number of key-value pairs stored
     */
    size_t size() const;

    /**
     * @brief Check if storage is empty
     * @return true if no items are stored
     */
    bool empty() const;

private:
    std::unordered_map<std::string, std::string> data_;
    mutable size_t access_count_ = 0;  // Track access patterns for debugging
};

} // namespace TestProject