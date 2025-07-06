#include "MemoryStorage.hpp"
#include "StorageBackend.hpp"
#include <algorithm>
#include <sstream>

namespace TestProject {

MemoryStorage::MemoryStorage() {
    // Initialize with empty storage
}

bool MemoryStorage::store(const std::string& key, const std::string& value) {
    data_[key] = value;
    ++access_count_;
    return true;
}

std::string MemoryStorage::retrieve(const std::string& key) const {
    ++access_count_;
    auto it = data_.find(key);
    return (it != data_.end()) ? it->second : "";
}

bool MemoryStorage::remove(const std::string& key) {
    ++access_count_;
    return data_.erase(key) > 0;
}

std::vector<std::string> MemoryStorage::listKeys() const {
    ++access_count_;
    std::vector<std::string> keys;
    keys.reserve(data_.size());
    
    for (const auto& pair : data_) {
        keys.push_back(pair.first);
    }
    
    // Sort keys for consistent output
    std::sort(keys.begin(), keys.end());
    return keys;
}

void MemoryStorage::clear() {
    ++access_count_;
    data_.clear();
}

std::string MemoryStorage::getBackendType() const {
    return "MemoryStorage";
}

size_t MemoryStorage::size() const {
    return data_.size();
}

bool MemoryStorage::empty() const {
    return data_.empty();
}

#ifdef ENABLE_DEBUG_LOGGING
std::string MemoryStorage::getDebugInfo() const {
    std::ostringstream oss;
    oss << "MemoryStorage Debug Info:\n";
    oss << "  Total entries: " << data_.size() << "\n";
    oss << "  Access count: " << access_count_ << "\n";
    oss << "  Memory efficiency: High (no I/O overhead)\n";
    oss << "  Persistence: None (data lost on program exit)\n";
    
    if (!data_.empty()) {
        oss << "  Sample entries:\n";
        size_t count = 0;
        for (const auto& pair : data_) {
            if (count >= 3) break;  // Show only first 3 entries
            oss << "    \"" << pair.first << "\" -> \"" << pair.second << "\"\n";
            ++count;
        }
    }
    
    return oss.str();
}
#endif

// Factory method implementation
std::unique_ptr<IStorageBackend> StorageBackend::create() {
    return std::make_unique<MemoryStorage>();
}

} // namespace TestProject