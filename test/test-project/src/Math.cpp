#include "Math.hpp"
#include <stdexcept>
#include <numeric>
#include <algorithm>
#include <cmath>

namespace TestProject {

long long Math::factorial(int n) {
    if (n < 0) {
        throw std::invalid_argument("Factorial is not defined for negative numbers");
    }
    
    if (n == 0 || n == 1) {
        return 1;
    }
    
    long long result = 1;
    for (int i = 2; i <= n; ++i) {
        result *= i;
    }
    return result;
}

long long Math::factorial(unsigned int n) {
    return factorial(static_cast<int>(n));
}

double Math::factorial(double n) {
    // For real numbers, use gamma function approximation: Γ(n+1) = n!
    // This is a simple approximation - for a real implementation, use tgamma
    if (n < 0) {
        throw std::invalid_argument("Factorial is not defined for negative numbers");
    }
    return std::tgamma(n + 1.0);
}

int Math::gcd(int a, int b) {
    a = std::abs(a);
    b = std::abs(b);
    
    while (b != 0) {
        int temp = b;
        b = a % b;
        a = temp;
    }
    return a;
}

long long Math::gcd(long long a, long long b) {
    a = std::abs(a);
    b = std::abs(b);
    
    while (b != 0) {
        long long temp = b;
        b = a % b;
        a = temp;
    }
    return a;
}

double Math::mean(const std::vector<double>& values) {
    if (values.empty()) {
        throw std::invalid_argument("Cannot calculate mean of empty vector");
    }
    
    double sum = std::accumulate(values.begin(), values.end(), 0.0);
    return sum / values.size();
}

double Math::mean(const std::vector<int>& values) {
    if (values.empty()) {
        throw std::invalid_argument("Cannot calculate mean of empty vector");
    }
    
    double sum = std::accumulate(values.begin(), values.end(), 0.0);
    return sum / values.size();
}

float Math::mean(const std::vector<float>& values) {
    if (values.empty()) {
        throw std::invalid_argument("Cannot calculate mean of empty vector");
    }
    
    float sum = std::accumulate(values.begin(), values.end(), 0.0f);
    return sum / values.size();
}

double Math::variance(const std::vector<double>& values) {
    if (values.empty()) {
        return 0.0;
    }
    
    double avg = mean(values);
    double sum_squared_diff = 0.0;
    
    for (const auto& value : values) {
        double diff = value - avg;
        sum_squared_diff += diff * diff;
    }
    
    return sum_squared_diff / values.size();
}

double Math::standardDeviation(const std::vector<double>& values) {
    return std::sqrt(variance(values));
}

double Math::standardDeviation(const std::vector<int>& values) {
    if (values.empty()) {
        return 0.0;
    }
    
    double avg = mean(values);
    double sum_sq_diff = 0.0;
    for (int val : values) {
        double diff = val - avg;
        sum_sq_diff += diff * diff;
    }
    return std::sqrt(sum_sq_diff / values.size());
}

bool Math::isPrime(int n) {
    if (n <= 1) {
        return false;
    }
    
    if (n <= 3) {
        return true;
    }
    
    if (n % 2 == 0 || n % 3 == 0) {
        return false;
    }
    
    for (int i = 5; i * i <= n; i += 6) {
        if (n % i == 0 || n % (i + 2) == 0) {
            return false;
        }
    }
    
    return true;
}

bool Math::isPrime(long long n) {
    if (n <= 1) {
        return false;
    }
    if (n <= 3) {
        return true;
    }
    if (n % 2 == 0 || n % 3 == 0) {
        return false;
    }
    
    for (long long i = 5; i * i <= n; i += 6) {
        if (n % i == 0 || n % (i + 2) == 0) {
            return false;
        }
    }
    return true;
}

bool Math::isPrime(unsigned int n) {
    return isPrime(static_cast<long long>(n));
}

double Math::power(double base, double exp) {
    return std::pow(base, exp);
}

int Math::power(int base, int exp) {
    if (exp < 0) {
        throw std::invalid_argument("Integer power with negative exponent");
    }
    int result = 1;
    for (int i = 0; i < exp; ++i) {
        result *= base;
    }
    return result;
}

double Math::log(double x) {
    if (x <= 0) {
        throw std::invalid_argument("Logarithm undefined for non-positive numbers");
    }
    return std::log(x);
}

double Math::log(double x, double base) {
    if (x <= 0 || base <= 0 || base == 1) {
        throw std::invalid_argument("Invalid arguments for logarithm");
    }
    return std::log(x) / std::log(base);
}

double Math::sqrt(double x) {
    if (x < 0) {
        throw std::invalid_argument("Square root of negative number");
    }
    return std::sqrt(x);
}

double Math::nthRoot(double x, int n) {
    if (n == 0) {
        throw std::invalid_argument("Zero-th root is undefined");
    }
    if (n % 2 == 0 && x < 0) {
        throw std::invalid_argument("Even root of negative number");
    }
    return std::pow(x, 1.0 / n);
}

double Math::sin(double x) {
    return std::sin(x);
}

double Math::cos(double x) {
    return std::cos(x);
}

double Math::tan(double x) {
    return std::tan(x);
}

int Math::min(int a, int b) {
    return std::min(a, b);
}

double Math::max(double a, double b) {
    return std::max(a, b);
}

// Complex number operations
Math::Complex::complex_t Math::Complex::add(const complex_t& a, const complex_t& b) {
    return a + b;
}

Math::Complex::complex_t Math::Complex::multiply(const complex_t& a, const complex_t& b) {
    return a * b;
}

Math::Complex::complex_t Math::Complex::divide(const complex_t& a, const complex_t& b) {
    if (std::abs(b) == 0.0) {
        throw std::invalid_argument("Division by zero complex number");
    }
    return a / b;
}

// Statistics operations
Math::Statistics::Result Math::Statistics::analyze(const std::vector<double>& values) {
    Result result;
    
    if (values.empty()) {
        return result;
    }
    
    result.count = values.size();
    
    // Calculate mean
    double sum = std::accumulate(values.begin(), values.end(), 0.0);
    result.mean = sum / result.count;
    
    // Calculate variance and standard deviation
    double sum_sq_diff = 0.0;
    for (double val : values) {
        double diff = val - result.mean;
        sum_sq_diff += diff * diff;
    }
    result.variance = sum_sq_diff / result.count;
    result.standard_deviation = std::sqrt(result.variance);
    
    // Find min and max
    auto minmax = std::minmax_element(values.begin(), values.end());
    result.min = *minmax.first;
    result.max = *minmax.second;
    
    // Calculate median
    std::vector<double> sorted_values = values;
    std::sort(sorted_values.begin(), sorted_values.end());
    size_t mid = result.count / 2;
    if (result.count % 2 == 0) {
        result.median = (sorted_values[mid - 1] + sorted_values[mid]) / 2.0;
    } else {
        result.median = sorted_values[mid];
    }
    
    return result;
}

} // namespace TestProject