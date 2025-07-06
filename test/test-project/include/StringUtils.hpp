#pragma once

#include <string>
#include <vector>
#include <map>

namespace TestProject {

/**
 * @brief String utility class with various string manipulation methods
 */
class StringUtils {
public:
    /**
     * @brief Convert string to uppercase
     * @param str Input string
     * @return Uppercase version of the string
     */
    static std::string toUpper(const std::string& str);

    /**
     * @brief Convert string to lowercase
     * @param str Input string
     * @return Lowercase version of the string
     */
    static std::string toLower(const std::string& str);

    /**
     * @brief Trim whitespace from both ends of a string
     * @param str Input string
     * @return Trimmed string
     */
    static std::string trim(const std::string& str);

    /**
     * @brief Split string by delimiter
     * @param str Input string
     * @param delimiter Character to split by
     * @return Vector of string tokens
     */
    static std::vector<std::string> split(const std::string& str, char delimiter);

    /**
     * @brief Join vector of strings with delimiter
     * @param tokens Vector of strings to join
     * @param delimiter Character to join with
     * @return Joined string
     */
    static std::string join(const std::vector<std::string>& tokens, char delimiter);

    /**
     * @brief Replace all occurrences of a substring
     * @param str Input string
     * @param from Substring to replace
     * @param to Replacement string
     * @return String with replacements made
     */
    static std::string replace(const std::string& str, const std::string& from, const std::string& to);

    /**
     * @brief Check if string starts with prefix
     * @param str Input string
     * @param prefix Prefix to check for
     * @return true if string starts with prefix
     */
    static bool startsWith(const std::string& str, const std::string& prefix);

    /**
     * @brief Check if string ends with suffix
     * @param str Input string
     * @param suffix Suffix to check for
     * @return true if string ends with suffix
     */
    static bool endsWith(const std::string& str, const std::string& suffix);

    /**
     * @brief Count character frequencies in a string
     * @param str Input string
     * @return Map of character to frequency count
     */
    static std::map<char, int> characterFrequency(const std::string& str);

private:
    // Private helper method to check if character is whitespace
    static bool isWhitespace(char c);
};

} // namespace TestProject