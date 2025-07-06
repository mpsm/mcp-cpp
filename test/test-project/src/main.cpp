#include <iostream>
#include <vector>
#include <string>
#include "Math.hpp"
#include "StringUtils.hpp"

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
    
    std::cout << "\n=== Demo Complete ===" << std::endl;
    return 0;
}