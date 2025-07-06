#pragma once

#include "IStorageBackend.hpp"
#include <string>
#include <vector>
#include <fstream>
#include <memory>
#include <unordered_map>

namespace TestProject {

/**
 * @brief File-based storage implementation
 * 
 * This implementation stores data in a simple text file format.
 * Data persists between program runs but has slower access times.
 */
class FileStorage : public IStorageBackend {
public:
    /**
     * @brief Construct a new FileStorage instance
     * @param filename The file to use for storage (default: "storage.txt")
     */
    explicit FileStorage(const std::string& filename = "storage.txt");

    /**
     * @brief Destructor - ensures data is flushed to disk
     */
    ~FileStorage() override;

    /**
     * @brief Store a key-value pair to file
     * @param key The key to store
     * @param value The value to associate with the key
     * @return true if successful, false on I/O error
     */
    bool store(const std::string& key, const std::string& value) override;

    /**
     * @brief Retrieve a value by key from file
     * @param key The key to look up
     * @return The associated value, or empty string if not found
     */
    std::string retrieve(const std::string& key) const override;

    /**
     * @brief Remove a key-value pair from file
     * @param key The key to remove
     * @return true if the key was found and removed, false otherwise
     */
    bool remove(const std::string& key) override;

    /**
     * @brief List all stored keys in file
     * @return Vector of all keys in the storage
     */
    std::vector<std::string> listKeys() const override;

    /**
     * @brief Clear all stored data from file
     */
    void clear() override;

    /**
     * @brief Get the backend type name
     * @return String identifying this as file storage
     */
    std::string getBackendType() const override;

#ifdef ENABLE_DEBUG_LOGGING
    /**
     * @brief Get debug information about file storage state
     * @return Debug information string
     */
    std::string getDebugInfo() const override;
#endif

    /**
     * @brief Get the storage filename
     * @return The filename used for storage
     */
    const std::string& getFilename() const;

    /**
     * @brief Check if the storage file exists
     * @return true if the file exists and is accessible
     */
    bool fileExists() const;

    /**
     * @brief Flush any pending changes to disk
     * @return true if successful, false on I/O error
     */
    bool flush();

private:
    std::string filename_;
    mutable size_t read_count_ = 0;   // Track read operations for debugging
    mutable size_t write_count_ = 0;  // Track write operations for debugging

    /**
     * @brief Load all data from file into memory temporarily
     * @return Map of key-value pairs from file
     */
    std::unordered_map<std::string, std::string> loadFromFile() const;

    /**
     * @brief Save all data from memory map to file
     * @param data The data to save
     * @return true if successful, false on I/O error
     */
    bool saveToFile(const std::unordered_map<std::string, std::string>& data);

    /**
     * @brief Escape special characters in strings for file storage
     * @param str The string to escape
     * @return Escaped string
     */
    std::string escapeString(const std::string& str) const;

    /**
     * @brief Unescape special characters from file storage
     * @param str The string to unescape
     * @return Unescaped string
     */
    std::string unescapeString(const std::string& str) const;
};

} // namespace TestProject