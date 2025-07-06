#include "FileStorage.hpp"
#include "StorageBackend.hpp"
#include <fstream>
#include <sstream>
#include <algorithm>
#include <unordered_map>

namespace TestProject {

FileStorage::FileStorage(const std::string& filename) : filename_(filename) {
    // Constructor - file will be created on first write if it doesn't exist
}

FileStorage::~FileStorage() {
    // Destructor - nothing special needed, file operations are immediate
}

bool FileStorage::store(const std::string& key, const std::string& value) {
    auto data = loadFromFile();
    data[key] = value;
    ++write_count_;
    return saveToFile(data);
}

std::string FileStorage::retrieve(const std::string& key) const {
    ++read_count_;
    auto data = loadFromFile();
    auto it = data.find(key);
    return (it != data.end()) ? it->second : "";
}

bool FileStorage::remove(const std::string& key) {
    auto data = loadFromFile();
    if (data.erase(key) > 0) {
        ++write_count_;
        return saveToFile(data);
    }
    return false;
}

std::vector<std::string> FileStorage::listKeys() const {
    ++read_count_;
    auto data = loadFromFile();
    std::vector<std::string> keys;
    keys.reserve(data.size());
    
    for (const auto& pair : data) {
        keys.push_back(pair.first);
    }
    
    // Sort keys for consistent output
    std::sort(keys.begin(), keys.end());
    return keys;
}

void FileStorage::clear() {
    ++write_count_;
    std::unordered_map<std::string, std::string> empty_data;
    saveToFile(empty_data);
}

std::string FileStorage::getBackendType() const {
    return "FileStorage";
}

const std::string& FileStorage::getFilename() const {
    return filename_;
}

bool FileStorage::fileExists() const {
    std::ifstream file(filename_);
    return file.good();
}

bool FileStorage::flush() {
    // For this implementation, all operations are immediately flushed
    return true;
}

std::unordered_map<std::string, std::string> FileStorage::loadFromFile() const {
    std::unordered_map<std::string, std::string> data;
    std::ifstream file(filename_);
    
    if (!file.is_open()) {
        return data;  // Return empty map if file doesn't exist
    }
    
    std::string line;
    while (std::getline(file, line)) {
        if (line.empty()) continue;
        
        // Simple format: key=value
        size_t eq_pos = line.find('=');
        if (eq_pos != std::string::npos) {
            std::string key = unescapeString(line.substr(0, eq_pos));
            std::string value = unescapeString(line.substr(eq_pos + 1));
            data[key] = value;
        }
    }
    
    return data;
}

bool FileStorage::saveToFile(const std::unordered_map<std::string, std::string>& data) {
    std::ofstream file(filename_);
    
    if (!file.is_open()) {
        return false;
    }
    
    for (const auto& pair : data) {
        file << escapeString(pair.first) << "=" << escapeString(pair.second) << "\n";
    }
    
    return file.good();
}

std::string FileStorage::escapeString(const std::string& str) const {
    std::string escaped;
    escaped.reserve(str.length());
    
    for (char c : str) {
        switch (c) {
            case '\n':
                escaped += "\\n";
                break;
            case '\r':
                escaped += "\\r";
                break;
            case '\t':
                escaped += "\\t";
                break;
            case '\\':
                escaped += "\\\\";
                break;
            case '=':
                escaped += "\\=";
                break;
            default:
                escaped += c;
                break;
        }
    }
    
    return escaped;
}

std::string FileStorage::unescapeString(const std::string& str) const {
    std::string unescaped;
    unescaped.reserve(str.length());
    
    for (size_t i = 0; i < str.length(); ++i) {
        if (str[i] == '\\' && i + 1 < str.length()) {
            switch (str[i + 1]) {
                case 'n':
                    unescaped += '\n';
                    ++i;
                    break;
                case 'r':
                    unescaped += '\r';
                    ++i;
                    break;
                case 't':
                    unescaped += '\t';
                    ++i;
                    break;
                case '\\':
                    unescaped += '\\';
                    ++i;
                    break;
                case '=':
                    unescaped += '=';
                    ++i;
                    break;
                default:
                    unescaped += str[i];
                    break;
            }
        } else {
            unescaped += str[i];
        }
    }
    
    return unescaped;
}

#ifdef ENABLE_DEBUG_LOGGING
std::string FileStorage::getDebugInfo() const {
    std::ostringstream oss;
    oss << "FileStorage Debug Info:\n";
    oss << "  Filename: " << filename_ << "\n";
    oss << "  File exists: " << (fileExists() ? "Yes" : "No") << "\n";
    oss << "  Read operations: " << read_count_ << "\n";
    oss << "  Write operations: " << write_count_ << "\n";
    oss << "  Persistence: Full (data survives program restart)\n";
    
    auto data = loadFromFile();
    oss << "  Total entries: " << data.size() << "\n";
    
    if (!data.empty()) {
        oss << "  Sample entries:\n";
        size_t count = 0;
        for (const auto& pair : data) {
            if (count >= 3) break;  // Show only first 3 entries
            oss << "    \"" << pair.first << "\" -> \"" << pair.second << "\"\n";
            ++count;
        }
    }
    
    return oss.str();
}
#endif

#ifndef USE_MEMORY_STORAGE
// Factory method implementation for file storage
std::unique_ptr<IStorageBackend> StorageBackend::create() {
    return std::make_unique<FileStorage>();
}
#endif

} // namespace TestProject