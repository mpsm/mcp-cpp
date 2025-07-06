#pragma once

#include <vector>
#include <cmath>

namespace TestProject {

/**
 * @brief Mathematical utility class with various calculation methods
 */
class Math {
public:
    /**
     * @brief Calculate the factorial of a number
     * @param n The number to calculate factorial for
     * @return The factorial of n
     */
    static long long factorial(int n);

    /**
     * @brief Calculate the greatest common divisor of two numbers
     * @param a First number
     * @param b Second number
     * @return The GCD of a and b
     */
    static int gcd(int a, int b);

    /**
     * @brief Calculate the mean of a vector of numbers
     * @param values Vector of numbers
     * @return The mean value
     */
    static double mean(const std::vector<double>& values);

    /**
     * @brief Calculate the standard deviation of a vector of numbers
     * @param values Vector of numbers
     * @return The standard deviation
     */
    static double standardDeviation(const std::vector<double>& values);

    /**
     * @brief Check if a number is prime
     * @param n Number to check
     * @return true if prime, false otherwise
     */
    static bool isPrime(int n);

private:
    // Private helper method for standard deviation calculation
    static double variance(const std::vector<double>& values);
};

} // namespace TestProject