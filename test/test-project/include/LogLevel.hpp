#pragma once

#include <string>
#include <ostream>
#include <unordered_map>
#include <vector>

namespace TestProject {

// Modern enum class for log levels
enum class LogLevel : int {
    TRACE = 0,
    DEBUG = 1,
    INFO = 2,
    WARNING = 3,
    ERROR = 4,
    CRITICAL = 5,
    OFF = 6
};

// Enum class for log output format
enum class LogFormat {
    PLAIN,
    JSON,
    XML,
    CSV
};

// Enum class for log destinations
enum class LogDestination {
    CONSOLE,
    FILE,
    SYSLOG,
    NETWORK
};

// Enum class with explicit values for configuration flags
enum class LogFlags : unsigned int {
    NONE = 0,
    TIMESTAMP = 1 << 0,
    THREAD_ID = 1 << 1,
    FUNCTION_NAME = 1 << 2,
    LINE_NUMBER = 1 << 3,
    MODULE_NAME = 1 << 4,
    COLORS = 1 << 5,
    ALL = TIMESTAMP | THREAD_ID | FUNCTION_NAME | LINE_NUMBER | MODULE_NAME | COLORS
};

// Bitwise operators for LogFlags
constexpr LogFlags operator|(LogFlags lhs, LogFlags rhs) {
    return static_cast<LogFlags>(
        static_cast<unsigned int>(lhs) | static_cast<unsigned int>(rhs)
    );
}

constexpr LogFlags operator&(LogFlags lhs, LogFlags rhs) {
    return static_cast<LogFlags>(
        static_cast<unsigned int>(lhs) & static_cast<unsigned int>(rhs)
    );
}

constexpr LogFlags operator^(LogFlags lhs, LogFlags rhs) {
    return static_cast<LogFlags>(
        static_cast<unsigned int>(lhs) ^ static_cast<unsigned int>(rhs)
    );
}

constexpr LogFlags operator~(LogFlags flags) {
    return static_cast<LogFlags>(~static_cast<unsigned int>(flags));
}

constexpr LogFlags& operator|=(LogFlags& lhs, LogFlags rhs) {
    lhs = lhs | rhs;
    return lhs;
}

constexpr LogFlags& operator&=(LogFlags& lhs, LogFlags rhs) {
    lhs = lhs & rhs;
    return lhs;
}

constexpr LogFlags& operator^=(LogFlags& lhs, LogFlags rhs) {
    lhs = lhs ^ rhs;
    return lhs;
}

// Utility functions for LogLevel
constexpr bool is_valid_log_level(LogLevel level) {
    return level >= LogLevel::TRACE && level <= LogLevel::OFF;
}

constexpr bool should_log(LogLevel message_level, LogLevel threshold_level) {
    return message_level >= threshold_level && threshold_level != LogLevel::OFF;
}

constexpr LogLevel get_default_log_level() {
    return LogLevel::INFO;
}

// String conversion functions
std::string to_string(LogLevel level);
std::string to_string(LogFormat format);
std::string to_string(LogDestination destination);
std::string to_string(LogFlags flags);

// Parsing functions
LogLevel parse_log_level(const std::string& level_str);
LogFormat parse_log_format(const std::string& format_str);
LogDestination parse_log_destination(const std::string& dest_str);
LogFlags parse_log_flags(const std::string& flags_str);

// Stream operators
std::ostream& operator<<(std::ostream& os, LogLevel level);
std::ostream& operator<<(std::ostream& os, LogFormat format);
std::ostream& operator<<(std::ostream& os, LogDestination destination);
std::ostream& operator<<(std::ostream& os, LogFlags flags);

// Configuration structure using enums
struct LogConfiguration {
    LogLevel level = LogLevel::INFO;
    LogFormat format = LogFormat::PLAIN;
    LogDestination destination = LogDestination::CONSOLE;
    LogFlags flags = LogFlags::TIMESTAMP | LogFlags::THREAD_ID;
    
    // Default constructor
    LogConfiguration() = default;
    
    // Constructor with custom values
    LogConfiguration(LogLevel lvl, LogFormat fmt, LogDestination dest, LogFlags flgs)
        : level(lvl), format(fmt), destination(dest), flags(flgs) {}
    
    // Copy constructor
    LogConfiguration(const LogConfiguration& other) = default;
    
    // Move constructor
    LogConfiguration(LogConfiguration&& other) noexcept = default;
    
    // Assignment operators
    LogConfiguration& operator=(const LogConfiguration& other) = default;
    LogConfiguration& operator=(LogConfiguration&& other) noexcept = default;
    
    // Equality operators
    bool operator==(const LogConfiguration& other) const {
        return level == other.level &&
               format == other.format &&
               destination == other.destination &&
               flags == other.flags;
    }
    
    bool operator!=(const LogConfiguration& other) const {
        return !(*this == other);
    }
    
    // Utility methods
    bool has_flag(LogFlags flag) const {
        return (flags & flag) == flag;
    }
    
    void set_flag(LogFlags flag) {
        flags |= flag;
    }
    
    void clear_flag(LogFlags flag) {
        flags &= ~flag;
    }
    
    void toggle_flag(LogFlags flag) {
        flags ^= flag;
    }
    
    bool is_enabled_for(LogLevel message_level) const {
        return should_log(message_level, level);
    }
    
    std::string to_string() const;
};

// Factory functions for common configurations
LogConfiguration create_debug_config();
LogConfiguration create_production_config();
LogConfiguration create_development_config();
LogConfiguration create_minimal_config();

// Configuration validation
bool is_valid_configuration(const LogConfiguration& config);
std::string validate_configuration(const LogConfiguration& config);

// Enum-based logger class (simplified)
class Logger {
private:
    LogConfiguration config_;
    std::string name_;
    
public:
    explicit Logger(const std::string& name, const LogConfiguration& config = LogConfiguration{})
        : config_(config), name_(name) {}
    
    // Log methods using enum
    void log(LogLevel level, const std::string& message);
    void trace(const std::string& message) { log(LogLevel::TRACE, message); }
    void debug(const std::string& message) { log(LogLevel::DEBUG, message); }
    void info(const std::string& message) { log(LogLevel::INFO, message); }
    void warning(const std::string& message) { log(LogLevel::WARNING, message); }
    void error(const std::string& message) { log(LogLevel::ERROR, message); }
    void critical(const std::string& message) { log(LogLevel::CRITICAL, message); }
    
    // Configuration methods
    void set_level(LogLevel level) { config_.level = level; }
    LogLevel get_level() const { return config_.level; }
    
    void set_format(LogFormat format) { config_.format = format; }
    LogFormat get_format() const { return config_.format; }
    
    void set_destination(LogDestination destination) { config_.destination = destination; }
    LogDestination get_destination() const { return config_.destination; }
    
    void set_flags(LogFlags flags) { config_.flags = flags; }
    LogFlags get_flags() const { return config_.flags; }
    
    const LogConfiguration& get_config() const { return config_; }
    void set_config(const LogConfiguration& config) { config_ = config; }
    
    bool is_enabled_for(LogLevel level) const {
        return config_.is_enabled_for(level);
    }
    
    const std::string& get_name() const { return name_; }
    void set_name(const std::string& name) { name_ = name; }
};

// Global logger registry using enum keys
class LoggerRegistry {
private:
    std::unordered_map<std::string, Logger> loggers_;
    LogConfiguration default_config_;
    
public:
    static LoggerRegistry& instance();
    
    Logger& get_logger(const std::string& name);
    void set_default_config(const LogConfiguration& config);
    const LogConfiguration& get_default_config() const;
    
    void set_global_level(LogLevel level);
    void set_global_format(LogFormat format);
    void set_global_destination(LogDestination destination);
    void set_global_flags(LogFlags flags);
    
    std::vector<std::string> get_logger_names() const;
    size_t get_logger_count() const;
    
    void clear_loggers();
    void shutdown();
};

// Helper macros for logging with enums
#define LOG_TRACE(logger, msg) if (logger.is_enabled_for(TestProject::LogLevel::TRACE)) logger.trace(msg)
#define LOG_DEBUG(logger, msg) if (logger.is_enabled_for(TestProject::LogLevel::DEBUG)) logger.debug(msg)
#define LOG_INFO(logger, msg) if (logger.is_enabled_for(TestProject::LogLevel::INFO)) logger.info(msg)
#define LOG_WARNING(logger, msg) if (logger.is_enabled_for(TestProject::LogLevel::WARNING)) logger.warning(msg)
#define LOG_ERROR(logger, msg) if (logger.is_enabled_for(TestProject::LogLevel::ERROR)) logger.error(msg)
#define LOG_CRITICAL(logger, msg) if (logger.is_enabled_for(TestProject::LogLevel::CRITICAL)) logger.critical(msg)

} // namespace TestProject