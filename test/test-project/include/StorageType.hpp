#pragma once

#include <string>
#include <ostream>
#include <vector>

namespace TestProject {

// Traditional enum for storage types (C-style enum)
enum StorageType {
    STORAGE_NONE = 0,
    STORAGE_MEMORY = 1,
    STORAGE_FILE = 2,
    STORAGE_DATABASE = 3,
    STORAGE_NETWORK = 4,
    STORAGE_CACHE = 5,
    STORAGE_HYBRID = 6
};

// Traditional enum for storage access patterns
enum AccessPattern {
    ACCESS_SEQUENTIAL = 0,
    ACCESS_RANDOM = 1,
    ACCESS_APPEND_ONLY = 2,
    ACCESS_READ_ONLY = 3,
    ACCESS_WRITE_ONLY = 4,
    ACCESS_READ_WRITE = 5
};

// Traditional enum for storage synchronization
enum SyncMode {
    SYNC_NONE = 0,
    SYNC_IMMEDIATE = 1,
    SYNC_DEFERRED = 2,
    SYNC_PERIODIC = 3,
    SYNC_ON_CLOSE = 4
};

// Traditional enum for storage compression
enum CompressionType {
    COMPRESSION_NONE = 0,
    COMPRESSION_GZIP = 1,
    COMPRESSION_ZLIB = 2,
    COMPRESSION_LZ4 = 3,
    COMPRESSION_SNAPPY = 4,
    COMPRESSION_BROTLI = 5
};

// Traditional enum for storage encryption
enum EncryptionType {
    ENCRYPTION_NONE = 0,
    ENCRYPTION_AES128 = 1,
    ENCRYPTION_AES256 = 2,
    ENCRYPTION_RSA = 3,
    ENCRYPTION_CHACHA20 = 4
};

// Traditional enum for storage reliability levels
enum ReliabilityLevel {
    RELIABILITY_NONE = 0,
    RELIABILITY_BASIC = 1,
    RELIABILITY_STANDARD = 2,
    RELIABILITY_HIGH = 3,
    RELIABILITY_CRITICAL = 4
};

// Traditional enum for storage error codes
enum StorageError {
    ERROR_NONE = 0,
    ERROR_NOT_FOUND = 1,
    ERROR_ACCESS_DENIED = 2,
    ERROR_DISK_FULL = 3,
    ERROR_NETWORK_FAILURE = 4,
    ERROR_CORRUPTION = 5,
    ERROR_TIMEOUT = 6,
    ERROR_UNSUPPORTED = 7,
    ERROR_INVALID_FORMAT = 8,
    ERROR_LOCK_FAILURE = 9,
    ERROR_UNKNOWN = 999
};

// Storage configuration structure using traditional enums
struct StorageConfig {
    StorageType type;
    AccessPattern access_pattern;
    SyncMode sync_mode;
    CompressionType compression;
    EncryptionType encryption;
    ReliabilityLevel reliability;
    
    // Default constructor
    StorageConfig() 
        : type(STORAGE_MEMORY),
          access_pattern(ACCESS_READ_WRITE),
          sync_mode(SYNC_IMMEDIATE),
          compression(COMPRESSION_NONE),
          encryption(ENCRYPTION_NONE),
          reliability(RELIABILITY_STANDARD) {}
    
    // Parameterized constructor
    StorageConfig(StorageType st, AccessPattern ap, SyncMode sm, 
                  CompressionType ct, EncryptionType et, ReliabilityLevel rl)
        : type(st), access_pattern(ap), sync_mode(sm), 
          compression(ct), encryption(et), reliability(rl) {}
    
    // Copy constructor
    StorageConfig(const StorageConfig& other) = default;
    
    // Assignment operator
    StorageConfig& operator=(const StorageConfig& other) = default;
    
    // Equality operators
    bool operator==(const StorageConfig& other) const {
        return type == other.type &&
               access_pattern == other.access_pattern &&
               sync_mode == other.sync_mode &&
               compression == other.compression &&
               encryption == other.encryption &&
               reliability == other.reliability;
    }
    
    bool operator!=(const StorageConfig& other) const {
        return !(*this == other);
    }
    
    // Utility methods
    bool is_encrypted() const {
        return encryption != ENCRYPTION_NONE;
    }
    
    bool is_compressed() const {
        return compression != COMPRESSION_NONE;
    }
    
    bool is_persistent() const {
        return type != STORAGE_MEMORY && type != STORAGE_CACHE;
    }
    
    bool is_networked() const {
        return type == STORAGE_NETWORK || type == STORAGE_DATABASE;
    }
    
    bool supports_random_access() const {
        return access_pattern == ACCESS_RANDOM || access_pattern == ACCESS_READ_WRITE;
    }
    
    bool is_readonly() const {
        return access_pattern == ACCESS_READ_ONLY;
    }
    
    bool is_writeonly() const {
        return access_pattern == ACCESS_WRITE_ONLY;
    }
    
    // Configuration validation
    bool is_valid() const;
    std::string get_validation_errors() const;
    
    // String representation
    std::string to_string() const;
};

// Utility functions for traditional enums
const char* storage_type_to_string(StorageType type);
const char* access_pattern_to_string(AccessPattern pattern);
const char* sync_mode_to_string(SyncMode mode);
const char* compression_type_to_string(CompressionType type);
const char* encryption_type_to_string(EncryptionType type);
const char* reliability_level_to_string(ReliabilityLevel level);
const char* storage_error_to_string(StorageError error);

// Parsing functions for traditional enums
StorageType string_to_storage_type(const char* str);
AccessPattern string_to_access_pattern(const char* str);
SyncMode string_to_sync_mode(const char* str);
CompressionType string_to_compression_type(const char* str);
EncryptionType string_to_encryption_type(const char* str);
ReliabilityLevel string_to_reliability_level(const char* str);
StorageError string_to_storage_error(const char* str);

// Stream operators for traditional enums
std::ostream& operator<<(std::ostream& os, StorageType type);
std::ostream& operator<<(std::ostream& os, AccessPattern pattern);
std::ostream& operator<<(std::ostream& os, SyncMode mode);
std::ostream& operator<<(std::ostream& os, CompressionType type);
std::ostream& operator<<(std::ostream& os, EncryptionType type);
std::ostream& operator<<(std::ostream& os, ReliabilityLevel level);
std::ostream& operator<<(std::ostream& os, StorageError error);
std::ostream& operator<<(std::ostream& os, const StorageConfig& config);

// Enum iteration support
std::vector<StorageType> get_all_storage_types();
std::vector<AccessPattern> get_all_access_patterns();
std::vector<SyncMode> get_all_sync_modes();
std::vector<CompressionType> get_all_compression_types();
std::vector<EncryptionType> get_all_encryption_types();
std::vector<ReliabilityLevel> get_all_reliability_levels();
std::vector<StorageError> get_all_storage_errors();

// Configuration factory functions
StorageConfig create_memory_config();
StorageConfig create_file_config();
StorageConfig create_database_config();
StorageConfig create_network_config();
StorageConfig create_cache_config();
StorageConfig create_hybrid_config();

// Configuration for specific use cases
StorageConfig create_high_performance_config();
StorageConfig create_high_security_config();
StorageConfig create_low_latency_config();
StorageConfig create_high_throughput_config();
StorageConfig create_space_efficient_config();

// Compatibility checking
bool is_compatible_config(const StorageConfig& config1, const StorageConfig& config2);
StorageConfig merge_configs(const StorageConfig& base, const StorageConfig& override);

// Performance hints based on enums
struct PerformanceHints {
    bool use_buffering;
    bool use_caching;
    bool use_compression;
    bool use_async_io;
    bool use_memory_mapping;
    size_t buffer_size;
    size_t cache_size;
    
    PerformanceHints() 
        : use_buffering(false), use_caching(false), use_compression(false),
          use_async_io(false), use_memory_mapping(false),
          buffer_size(0), cache_size(0) {}
};

PerformanceHints get_performance_hints(const StorageConfig& config);

// Error handling utilities
class StorageException {
private:
    StorageError error_code_;
    std::string message_;
    
public:
    StorageException(StorageError error, const std::string& message)
        : error_code_(error), message_(message) {}
    
    StorageError get_error_code() const { return error_code_; }
    const std::string& get_message() const { return message_; }
    
    std::string what() const {
        return std::string(storage_error_to_string(error_code_)) + ": " + message_;
    }
};

// Storage statistics using enums
struct StorageStats {
    StorageType type;
    size_t total_operations;
    size_t read_operations;
    size_t write_operations;
    size_t error_count;
    StorageError last_error;
    
    StorageStats() 
        : type(STORAGE_NONE), total_operations(0), read_operations(0),
          write_operations(0), error_count(0), last_error(ERROR_NONE) {}
    
    void increment_read() { ++read_operations; ++total_operations; }
    void increment_write() { ++write_operations; ++total_operations; }
    void record_error(StorageError error) { ++error_count; last_error = error; }
    
    double get_error_rate() const {
        return total_operations > 0 ? static_cast<double>(error_count) / total_operations : 0.0;
    }
    
    std::string to_string() const;
};

// Global storage registry using traditional enums
class StorageRegistry {
private:
    std::vector<StorageConfig> configs_;
    StorageStats global_stats_;
    
public:
    static StorageRegistry& instance();
    
    void register_config(const StorageConfig& config);
    std::vector<StorageConfig> get_configs_by_type(StorageType type) const;
    std::vector<StorageConfig> get_configs_by_pattern(AccessPattern pattern) const;
    
    StorageConfig get_default_config() const;
    void set_default_config(const StorageConfig& config);
    
    const StorageStats& get_global_stats() const;
    void update_stats(const StorageStats& stats);
    void reset_stats();
    
    size_t get_config_count() const;
    void clear_configs();
};

// Helper macros for storage operations
#define STORAGE_CHECK_TYPE(config, expected_type) \
    if ((config).type != (expected_type)) { \
        throw StorageException(ERROR_UNSUPPORTED, "Unsupported storage type"); \
    }

#define STORAGE_CHECK_ACCESS(config, required_access) \
    if ((config).access_pattern != (required_access) && (config).access_pattern != ACCESS_READ_WRITE) { \
        throw StorageException(ERROR_ACCESS_DENIED, "Access pattern not supported"); \
    }

#define STORAGE_VERIFY_CONFIG(config) \
    if (!(config).is_valid()) { \
        throw StorageException(ERROR_INVALID_FORMAT, (config).get_validation_errors()); \
    }

} // namespace TestProject