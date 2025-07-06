#include <iostream>
#include <vector>
#include <string>
#include "Math.hpp"
#include "StringUtils.hpp"
#include "StorageBackend.hpp"

using namespace TestProject;

int main() {
    std::cout << "=== TestProject Demo ===" << std::endl;
    
    // Math utility demonstrations
    std::cout << "\n--- Math Utilities ---" << std::endl;
    
    // Factorial
    int n = 5;
    std::cout << "Factorial of " << n << " = " << Math::factorial(n) << std::endl;
    
    // GCD
    int a = 48, b = 18;
    std::cout << "GCD of " << a << " and " << b << " = " << Math::gcd(a, b) << std::endl;
    
    // Statistical functions
    std::vector<double> numbers = {1.5, 2.5, 3.5, 4.5, 5.5, 6.5};
    std::cout << "Mean of numbers: " << Math::mean(numbers) << std::endl;
    std::cout << "Standard deviation: " << Math::standardDeviation(numbers) << std::endl;
    
    // Prime checking
    std::vector<int> primeTests = {17, 25, 29, 100};
    std::cout << "Prime number checks:" << std::endl;
    for (int num : primeTests) {
        std::cout << "  " << num << " is " << (Math::isPrime(num) ? "prime" : "not prime") << std::endl;
    }
    
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