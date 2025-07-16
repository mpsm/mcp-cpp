#include <iostream>
#include <vector>
#include <string>
#include <complex>
#include <array>
#include <functional>
#include <memory>
#include <chrono>
#include <iomanip>
#include "Math.hpp"
#include "StringUtils.hpp"
#include "StorageBackend.hpp"
#include "Container.hpp"
#include "Algorithms.hpp"
#include "LogLevel.hpp"
#include "StorageType.hpp"

using namespace TestProject;

int main() {
    std::cout << "=== Enhanced TestProject Demo ===" << std::endl;
    
    // Enhanced Math utility demonstrations with overloading
    std::cout << "\n--- Enhanced Math Utilities (Function Overloading) ---" << std::endl;
    
    // Factorial overloads
    int n = 5;
    unsigned int un = 6;
    double dn = 4.5;
    std::cout << "Factorial overloads:" << std::endl;
    std::cout << "  factorial(int " << n << ") = " << Math::factorial(n) << std::endl;
    std::cout << "  factorial(unsigned int " << un << ") = " << Math::factorial(un) << std::endl;
    std::cout << "  factorial(double " << dn << ") = " << Math::factorial(dn) << " (gamma function)" << std::endl;
    
    // GCD overloads
    int a = 48, b = 18;
    long long la = 12345LL, lb = 67890LL;
    std::cout << "\nGCD overloads:" << std::endl;
    std::cout << "  gcd(int " << a << ", int " << b << ") = " << Math::gcd(a, b) << std::endl;
    std::cout << "  gcd(long long " << la << ", long long " << lb << ") = " << Math::gcd(la, lb) << std::endl;
    
    // Statistical functions with overloads
    std::vector<double> numbers = {1.5, 2.5, 3.5, 4.5, 5.5, 6.5};
    std::vector<int> intNumbers = {1, 2, 3, 4, 5, 6};
    std::vector<float> floatNumbers = {1.1f, 2.2f, 3.3f, 4.4f, 5.5f};
    std::array<double, 5> arrayNumbers = {10.0, 20.0, 30.0, 40.0, 50.0};
    
    std::cout << "\nStatistical function overloads:" << std::endl;
    std::cout << "  mean(vector<double>): " << Math::mean(numbers) << std::endl;
    std::cout << "  mean(vector<int>): " << Math::mean(intNumbers) << std::endl;
    std::cout << "  mean(vector<float>): " << Math::mean(floatNumbers) << std::endl;
    std::cout << "  mean(array<double, 5>): " << Math::mean(arrayNumbers) << std::endl;
    
    std::cout << "  standardDeviation(vector<double>): " << Math::standardDeviation(numbers) << std::endl;
    std::cout << "  standardDeviation(vector<int>): " << Math::standardDeviation(intNumbers) << std::endl;
    
    // Prime checking overloads
    std::vector<int> primeTests = {17, 25, 29, 100};
    std::cout << "\nPrime number checks (overloads):" << std::endl;
    std::cout << "  isPrime(int 17): " << (Math::isPrime(17) ? "prime" : "not prime") << std::endl;
    std::cout << "  isPrime(long long 1000000007LL): " << (Math::isPrime(1000000007LL) ? "prime" : "not prime") << std::endl;
    std::cout << "  isPrime(unsigned int 997U): " << (Math::isPrime(997U) ? "prime" : "not prime") << std::endl;
    
    // Advanced math functions
    std::cout << "\nAdvanced math functions:" << std::endl;
    std::cout << "  power(2.0, 3.0): " << Math::power(2.0, 3.0) << std::endl;
    std::cout << "  power(2, 10): " << Math::power(2, 10) << std::endl;
    std::cout << "  log(Math::E): " << Math::log(Math::E) << std::endl;
    std::cout << "  log(8.0, 2.0): " << Math::log(8.0, 2.0) << std::endl;
    std::cout << "  sqrt(16.0): " << Math::sqrt(16.0) << std::endl;
    std::cout << "  nthRoot(27.0, 3): " << Math::nthRoot(27.0, 3) << std::endl;
    
    // Trigonometric functions
    std::cout << "\nTrigonometric functions:" << std::endl;
    std::cout << "  sin(π/2): " << Math::sin(Math::PI / 2) << std::endl;
    std::cout << "  cos(π): " << Math::cos(Math::PI) << std::endl;
    std::cout << "  tan(π/4): " << Math::tan(Math::PI / 4) << std::endl;
    
    // Min/Max overloads
    std::cout << "\nMin/Max function overloads:" << std::endl;
    std::cout << "  min(5, 10): " << Math::min(5, 10) << std::endl;
    std::cout << "  max(3.14, 2.71): " << Math::max(3.14, 2.71) << std::endl;
    std::cout << "  min({1, 5, 3, 9, 2}): " << Math::min({1, 5, 3, 9, 2}) << std::endl;
    std::cout << "  max(intNumbers): " << Math::max(intNumbers) << std::endl;
    
    // Math nested classes demonstration
    std::cout << "\n--- Math Nested Classes ---" << std::endl;
    
    // Statistics nested class
    auto stats = Math::Statistics::analyze(numbers);
    std::cout << "Statistics analysis:" << std::endl;
    std::cout << "  Mean: " << stats.mean << std::endl;
    std::cout << "  Variance: " << stats.variance << std::endl;
    std::cout << "  Std Dev: " << stats.standard_deviation << std::endl;
    std::cout << "  Median: " << stats.median << std::endl;
    std::cout << "  Min: " << stats.min << std::endl;
    std::cout << "  Max: " << stats.max << std::endl;
    std::cout << "  Count: " << stats.count << std::endl;
    
    // Complex number operations
    std::cout << "\nComplex number operations:" << std::endl;
    std::complex<double> c1(3.0, 4.0);
    std::complex<double> c2(1.0, 2.0);
    auto sum = Math::Complex::add(c1, c2);
    auto product = Math::Complex::multiply(c1, c2);
    auto quotient = Math::Complex::divide(c1, c2);
    
    std::cout << "  (3+4i) + (1+2i) = " << sum << std::endl;
    std::cout << "  (3+4i) * (1+2i) = " << product << std::endl;
    std::cout << "  (3+4i) / (1+2i) = " << quotient << std::endl;
    
    // Matrix operations
    std::cout << "\nMatrix operations:" << std::endl;
    Math::Matrix2x2 matrix1;
    matrix1(0, 0) = 1.0; matrix1(0, 1) = 2.0;
    matrix1(1, 0) = 3.0; matrix1(1, 1) = 4.0;
    
    Math::Matrix2x2 matrix2;
    matrix2(0, 0) = 5.0; matrix2(0, 1) = 6.0;
    matrix2(1, 0) = 7.0; matrix2(1, 1) = 8.0;
    
    auto matrixSum = matrix1 + matrix2;
    std::cout << "  Matrix addition result:" << std::endl;
    std::cout << "    [" << matrixSum(0, 0) << ", " << matrixSum(0, 1) << "]" << std::endl;
    std::cout << "    [" << matrixSum(1, 0) << ", " << matrixSum(1, 1) << "]" << std::endl;
    
    // Template Container demonstrations
    std::cout << "\n--- Template Container Operations ---" << std::endl;
    
    // Integer container
    Container<int> intContainer;
    intContainer.push_back(10);
    intContainer.push_back(20);
    intContainer.push_back(30);
    intContainer.push_back(40);
    intContainer.push_back(50);
    
    std::cout << "Integer container operations:" << std::endl;
    std::cout << "  Size: " << intContainer.size() << std::endl;
    std::cout << "  Elements: ";
    for (const auto& elem : intContainer) {
        std::cout << elem << " ";
    }
    std::cout << std::endl;
    
    // Transform operation
    auto doubledContainer = intContainer.transform([](int x) { return x * 2; });
    std::cout << "  Doubled: ";
    for (const auto& elem : doubledContainer) {
        std::cout << elem << " ";
    }
    std::cout << std::endl;
    
    // Count operations
    auto evenCount = intContainer.count_if([](int x) { return x % 2 == 0; });
    std::cout << "  Even numbers count: " << evenCount << std::endl;
    
    // Statistics for container
    auto containerStats = intContainer.compute_statistics();
    std::cout << "  Container statistics - Min: " << containerStats.min_value 
              << ", Max: " << containerStats.max_value 
              << ", Count: " << containerStats.count << std::endl;
    
    // String container
    Container<std::string> stringContainer;
    stringContainer.push_back("apple");
    stringContainer.push_back("banana");
    stringContainer.push_back("cherry");
    stringContainer.push_back("date");
    
    std::cout << "\nString container operations:" << std::endl;
    std::cout << "  Elements: ";
    for (const auto& elem : stringContainer) {
        std::cout << "\"" << elem << "\" ";
    }
    std::cout << std::endl;
    
    // Bool container specialization
    Container<bool> boolContainer;
    boolContainer.push_back(true);
    boolContainer.push_back(false);
    boolContainer.push_back(true);
    boolContainer.push_back(true);
    boolContainer.push_back(false);
    
    std::cout << "\nBool container specialization:" << std::endl;
    std::cout << "  True count: " << boolContainer.count_true() << std::endl;
    std::cout << "  False count: " << boolContainer.count_false() << std::endl;
    
    // Algorithm demonstrations
    std::cout << "\n--- Template Algorithm Operations ---" << std::endl;
    
    std::vector<int> algorithmData = {5, 2, 8, 1, 9, 3, 7, 4, 6};
    std::cout << "Original data: ";
    for (int val : algorithmData) {
        std::cout << val << " ";
    }
    std::cout << std::endl;
    
    // Find max element
    auto maxIt = Algorithms::max_element(algorithmData.begin(), algorithmData.end());
    std::cout << "Max element: " << *maxIt << std::endl;
    
    // Binary search (first sort)
    std::sort(algorithmData.begin(), algorithmData.end());
    bool found = Algorithms::binary_search(algorithmData.begin(), algorithmData.end(), 5);
    std::cout << "Binary search for 5: " << (found ? "found" : "not found") << std::endl;
    
    // Transform operation
    std::vector<int> transformedData;
    Algorithms::transform(algorithmData.begin(), algorithmData.end(), 
                         std::back_inserter(transformedData), 
                         [](int x) { return x * x; });
    std::cout << "Squared values: ";
    for (int val : transformedData) {
        std::cout << val << " ";
    }
    std::cout << std::endl;
    
    // Accumulate operation
    int arraySum = Algorithms::accumulate(algorithmData.begin(), algorithmData.end(), 0);
    std::cout << "Sum of elements: " << arraySum << std::endl;
    
    // Enum class demonstrations
    std::cout << "\n--- Enum Class Operations (LogLevel) ---" << std::endl;
    
    LogLevel currentLevel = LogLevel::INFO;
    std::cout << "Current log level: " << currentLevel << std::endl;
    
    LogConfiguration config;
    config.level = LogLevel::DEBUG;
    config.format = LogFormat::JSON;
    config.destination = LogDestination::FILE;
    config.flags = LogFlags::TIMESTAMP | LogFlags::THREAD_ID | LogFlags::FUNCTION_NAME;
    
    std::cout << "Log configuration:" << std::endl;
    std::cout << "  Level: " << config.level << std::endl;
    std::cout << "  Format: " << config.format << std::endl;
    std::cout << "  Destination: " << config.destination << std::endl;
    std::cout << "  Has timestamp flag: " << (config.has_flag(LogFlags::TIMESTAMP) ? "yes" : "no") << std::endl;
    std::cout << "  Has colors flag: " << (config.has_flag(LogFlags::COLORS) ? "yes" : "no") << std::endl;
    
    // Logger demonstration
    Logger logger("TestLogger", config);
    std::cout << "\nLogger operations:" << std::endl;
    std::cout << "  Logger name: " << logger.get_name() << std::endl;
    std::cout << "  Is enabled for DEBUG: " << (logger.is_enabled_for(LogLevel::DEBUG) ? "yes" : "no") << std::endl;
    std::cout << "  Is enabled for ERROR: " << (logger.is_enabled_for(LogLevel::ERROR) ? "yes" : "no") << std::endl;
    
    // Traditional enum demonstrations
    std::cout << "\n--- Traditional Enum Operations (StorageType) ---" << std::endl;
    
    StorageConfig storageConfig;
    storageConfig.type = STORAGE_DATABASE;
    storageConfig.access_pattern = ACCESS_READ_WRITE;
    storageConfig.sync_mode = SYNC_IMMEDIATE;
    storageConfig.compression = COMPRESSION_GZIP;
    storageConfig.encryption = ENCRYPTION_AES256;
    storageConfig.reliability = RELIABILITY_HIGH;
    
    std::cout << "Storage configuration:" << std::endl;
    std::cout << "  Type: " << storageConfig.type << std::endl;
    std::cout << "  Access pattern: " << storageConfig.access_pattern << std::endl;
    std::cout << "  Sync mode: " << storageConfig.sync_mode << std::endl;
    std::cout << "  Compression: " << storageConfig.compression << std::endl;
    std::cout << "  Encryption: " << storageConfig.encryption << std::endl;
    std::cout << "  Reliability: " << storageConfig.reliability << std::endl;
    
    std::cout << "\nStorage configuration properties:" << std::endl;
    std::cout << "  Is encrypted: " << (storageConfig.is_encrypted() ? "yes" : "no") << std::endl;
    std::cout << "  Is compressed: " << (storageConfig.is_compressed() ? "yes" : "no") << std::endl;
    std::cout << "  Is persistent: " << (storageConfig.is_persistent() ? "yes" : "no") << std::endl;
    std::cout << "  Is networked: " << (storageConfig.is_networked() ? "yes" : "no") << std::endl;
    std::cout << "  Supports random access: " << (storageConfig.supports_random_access() ? "yes" : "no") << std::endl;
    std::cout << "  Configuration is valid: " << (storageConfig.is_valid() ? "yes" : "no") << std::endl;
    
    // String utility demonstrations
    std::cout << "\n--- String Utilities ---" << std::endl;
    
    // Case conversion
    std::string testStr = "Hello World";
    std::cout << "Original: \"" << testStr << "\"" << std::endl;
    std::cout << "Uppercase: \"" << StringUtils::toUpper(testStr) << "\"" << std::endl;
    std::cout << "Lowercase: \"" << StringUtils::toLower(testStr) << "\"" << std::endl;
    
    // Trimming
    std::string whitespaceStr = "  \t  Hello World  \n  ";
    std::cout << "Trimmed: \"" << StringUtils::trim(whitespaceStr) << "\"" << std::endl;
    
    // String splitting and joining
    std::string csvData = "apple,banana,cherry,date";
    std::vector<std::string> fruits = StringUtils::split(csvData, ',');
    std::cout << "Split CSV: ";
    for (const auto& fruit : fruits) {
        std::cout << "\"" << fruit << "\" ";
    }
    std::cout << std::endl;
    
    std::string rejoined = StringUtils::join(fruits, '|');
    std::cout << "Rejoined with |: \"" << rejoined << "\"" << std::endl;
    
    // String replacement
    std::string replaceTest = "The quick brown fox jumps over the lazy dog";
    std::string replaced = StringUtils::replace(replaceTest, "fox", "cat");
    std::cout << "Replace 'fox' with 'cat': \"" << replaced << "\"" << std::endl;
    
    // Prefix/suffix checking
    std::string filename = "document.pdf";
    std::cout << "File \"" << filename << "\":" << std::endl;
    std::cout << "  Starts with 'doc': " << (StringUtils::startsWith(filename, "doc") ? "yes" : "no") << std::endl;
    std::cout << "  Ends with '.pdf': " << (StringUtils::endsWith(filename, ".pdf") ? "yes" : "no") << std::endl;
    
    // Character frequency
    std::string freqTest = "hello world";
    auto frequencies = StringUtils::characterFrequency(freqTest);
    std::cout << "Character frequencies in \"" << freqTest << "\":" << std::endl;
    for (const auto& pair : frequencies) {
        std::cout << "  '" << pair.first << "': " << pair.second << std::endl;
    }
    
    // Storage backend demonstration - showcases conditional compilation
    std::cout << "\n--- Storage Backend (Conditional Compilation) ---" << std::endl;
    
    // Create storage backend - the actual type depends on compile-time options
    auto storage = StorageBackend::create();
    std::cout << "Using backend: " << storage->getBackendType() << std::endl;
    
    // Store some test data
    std::vector<std::pair<std::string, std::string>> testData = {
        {"name", "John Doe"},
        {"age", "30"},
        {"city", "New York"},
        {"occupation", "Software Engineer"}
    };
    
    std::cout << "\nStoring test data..." << std::endl;
    for (const auto& pair : testData) {
        if (storage->store(pair.first, pair.second)) {
            std::cout << "  Stored: " << pair.first << " -> " << pair.second << std::endl;
        }
    }
    
    // Retrieve and display data
    std::cout << "\nRetrieving stored data:" << std::endl;
    auto keys = storage->listKeys();
    for (const auto& key : keys) {
        std::string value = storage->retrieve(key);
        std::cout << "  " << key << " = " << value << std::endl;
    }
    
    // Demonstrate conditional compilation with debug info
#ifdef ENABLE_DEBUG_LOGGING
    std::cout << "\n--- Debug Information (Conditional Feature) ---" << std::endl;
    std::cout << storage->getDebugInfo() << std::endl;
#else
    std::cout << "\nDebug logging is disabled (compile with -DENABLE_DEBUG_LOGGING=ON to enable)" << std::endl;
#endif
    
    // Conditional compilation demonstration in preprocessor output
    std::cout << "\n--- Compile-Time Configuration ---" << std::endl;
#ifdef USE_MEMORY_STORAGE
    std::cout << "Storage backend: Memory (fast, non-persistent)" << std::endl;
    std::cout << "Compile-time type: " << typeid(SelectedBackend).name() << std::endl;
#else
    std::cout << "Storage backend: File (persistent, slower)" << std::endl;
    std::cout << "Compile-time type: " << typeid(SelectedBackend).name() << std::endl;
#endif

#ifdef ENABLE_DEBUG_LOGGING
    std::cout << "Debug logging: Enabled" << std::endl;
#else
    std::cout << "Debug logging: Disabled" << std::endl;
#endif
    
    // Clean up
    std::cout << "\nCleaning up storage..." << std::endl;
    storage->clear();
    
    std::cout << "\n=== Demo Complete ===" << std::endl;
    return 0;
}