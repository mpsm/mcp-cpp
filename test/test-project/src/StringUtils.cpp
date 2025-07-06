#include "StringUtils.hpp"
#include <algorithm>
#include <cctype>
#include <sstream>

namespace TestProject {

std::string StringUtils::toUpper(const std::string& str) {
    std::string result = str;
    std::transform(result.begin(), result.end(), result.begin(), ::toupper);
    return result;
}

std::string StringUtils::toLower(const std::string& str) {
    std::string result = str;
    std::transform(result.begin(), result.end(), result.begin(), ::tolower);
    return result;
}

bool StringUtils::isWhitespace(char c) {
    return std::isspace(static_cast<unsigned char>(c));
}

std::string StringUtils::trim(const std::string& str) {
    if (str.empty()) {
        return str;
    }
    
    size_t start = 0;
    size_t end = str.length();
    
    // Find first non-whitespace character
    while (start < end && isWhitespace(str[start])) {
        ++start;
    }
    
    // Find last non-whitespace character
    while (end > start && isWhitespace(str[end - 1])) {
        --end;
    }
    
    return str.substr(start, end - start);
}

std::vector<std::string> StringUtils::split(const std::string& str, char delimiter) {
    std::vector<std::string> tokens;
    std::stringstream ss(str);
    std::string token;
    
    while (std::getline(ss, token, delimiter)) {
        tokens.push_back(token);
    }
    
    return tokens;
}

std::string StringUtils::join(const std::vector<std::string>& tokens, char delimiter) {
    if (tokens.empty()) {
        return "";
    }
    
    std::ostringstream oss;
    for (size_t i = 0; i < tokens.size(); ++i) {
        if (i > 0) {
            oss << delimiter;
        }
        oss << tokens[i];
    }
    
    return oss.str();
}

std::string StringUtils::replace(const std::string& str, const std::string& from, const std::string& to) {
    if (from.empty()) {
        return str;
    }
    
    std::string result = str;
    size_t pos = 0;
    
    while ((pos = result.find(from, pos)) != std::string::npos) {
        result.replace(pos, from.length(), to);
        pos += to.length();
    }
    
    return result;
}

bool StringUtils::startsWith(const std::string& str, const std::string& prefix) {
    if (prefix.length() > str.length()) {
        return false;
    }
    
    return str.substr(0, prefix.length()) == prefix;
}

bool StringUtils::endsWith(const std::string& str, const std::string& suffix) {
    if (suffix.length() > str.length()) {
        return false;
    }
    
    return str.substr(str.length() - suffix.length()) == suffix;
}

std::map<char, int> StringUtils::characterFrequency(const std::string& str) {
    std::map<char, int> frequency;
    
    for (char c : str) {
        frequency[c]++;
    }
    
    return frequency;
}

} // namespace TestProject