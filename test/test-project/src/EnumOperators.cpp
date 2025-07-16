#include "LogLevel.hpp"
#include "StorageType.hpp"
#include <ostream>

namespace TestProject {

// LogLevel stream operator
std::ostream& operator<<(std::ostream& os, LogLevel level) {
    switch (level) {
        case LogLevel::TRACE: return os << "TRACE";
        case LogLevel::DEBUG: return os << "DEBUG";
        case LogLevel::INFO: return os << "INFO";
        case LogLevel::WARNING: return os << "WARNING";
        case LogLevel::ERROR: return os << "ERROR";
        case LogLevel::CRITICAL: return os << "CRITICAL";
        case LogLevel::OFF: return os << "OFF";
        default: return os << "UNKNOWN";
    }
}

// StorageType stream operator
std::ostream& operator<<(std::ostream& os, StorageType type) {
    switch (type) {
        case STORAGE_NONE: return os << "NONE";
        case STORAGE_MEMORY: return os << "MEMORY";
        case STORAGE_FILE: return os << "FILE";
        case STORAGE_DATABASE: return os << "DATABASE";
        case STORAGE_NETWORK: return os << "NETWORK";
        case STORAGE_CACHE: return os << "CACHE";
        case STORAGE_HYBRID: return os << "HYBRID";
        default: return os << "UNKNOWN";
    }
}

// Stub implementations for other operators to satisfy linker
std::ostream& operator<<(std::ostream& os, LogFormat) { return os << "LogFormat"; }
std::ostream& operator<<(std::ostream& os, LogDestination) { return os << "LogDestination"; }
std::ostream& operator<<(std::ostream& os, AccessPattern) { return os << "AccessPattern"; }
std::ostream& operator<<(std::ostream& os, SyncMode) { return os << "SyncMode"; }
std::ostream& operator<<(std::ostream& os, CompressionType) { return os << "CompressionType"; }
std::ostream& operator<<(std::ostream& os, EncryptionType) { return os << "EncryptionType"; }
std::ostream& operator<<(std::ostream& os, ReliabilityLevel) { return os << "ReliabilityLevel"; }

// StorageConfig::is_valid implementation
bool StorageConfig::is_valid() const {
    // Basic validation - ensure enum values are within expected ranges
    // This is a simple implementation for the demo
    return true; // For now, consider all configurations valid
}

} // namespace TestProject
