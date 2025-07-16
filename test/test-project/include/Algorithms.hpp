#pragma once

#include <vector>
#include <algorithm>
#include <functional>
#include <iterator>
#include <type_traits>
#include <numeric>
#include <utility>
#include <execution>

namespace TestProject {
namespace Algorithms {

// Template function for finding maximum element with custom comparator
template<typename ForwardIt, typename Compare>
ForwardIt max_element(ForwardIt first, ForwardIt last, Compare comp) {
    if (first == last) return last;
    
    ForwardIt largest = first;
    ++first;
    
    for (; first != last; ++first) {
        if (comp(*largest, *first)) {
            largest = first;
        }
    }
    return largest;
}

// Overload with default comparator
template<typename ForwardIt>
ForwardIt max_element(ForwardIt first, ForwardIt last) {
    using value_type = typename std::iterator_traits<ForwardIt>::value_type;
    return TestProject::Algorithms::max_element(first, last, std::less<value_type>());
}

// Template function for binary search with custom comparator
template<typename ForwardIt, typename T, typename Compare>
bool binary_search(ForwardIt first, ForwardIt last, const T& value, Compare comp) {
    first = std::lower_bound(first, last, value, comp);
    return (first != last && !comp(value, *first));
}

// Overload with default comparator
template<typename ForwardIt, typename T>
bool binary_search(ForwardIt first, ForwardIt last, const T& value) {
    return TestProject::Algorithms::binary_search(first, last, value, std::less<T>());
}

// Template function for partitioning with custom predicate
template<typename ForwardIt, typename UnaryPredicate>
ForwardIt partition(ForwardIt first, ForwardIt last, UnaryPredicate pred) {
    first = std::find_if_not(first, last, pred);
    if (first == last) return first;
    
    for (ForwardIt it = std::next(first); it != last; ++it) {
        if (pred(*it)) {
            std::iter_swap(it, first);
            ++first;
        }
    }
    return first;
}

// Template function for stable partitioning
template<typename BidirIt, typename UnaryPredicate>
BidirIt stable_partition(BidirIt first, BidirIt last, UnaryPredicate pred) {
    return std::stable_partition(first, last, pred);
}

// Template function for merge operation
template<typename InputIt1, typename InputIt2, typename OutputIt, typename Compare>
OutputIt merge(InputIt1 first1, InputIt1 last1,
               InputIt2 first2, InputIt2 last2,
               OutputIt result, Compare comp) {
    while (first1 != last1 && first2 != last2) {
        if (comp(*first1, *first2)) {
            *result = *first1;
            ++first1;
        } else {
            *result = *first2;
            ++first2;
        }
        ++result;
    }
    
    result = std::copy(first1, last1, result);
    return std::copy(first2, last2, result);
}

// Overload with default comparator
template<typename InputIt1, typename InputIt2, typename OutputIt>
OutputIt merge(InputIt1 first1, InputIt1 last1,
               InputIt2 first2, InputIt2 last2,
               OutputIt result) {
    using value_type = typename std::iterator_traits<InputIt1>::value_type;
    return merge(first1, last1, first2, last2, result, std::less<value_type>());
}

// Template function for transforming elements
template<typename InputIt, typename OutputIt, typename UnaryOperation>
OutputIt transform(InputIt first, InputIt last, OutputIt result, UnaryOperation op) {
    while (first != last) {
        *result = op(*first);
        ++result;
        ++first;
    }
    return result;
}

// Binary transform operation
template<typename InputIt1, typename InputIt2, typename OutputIt, typename BinaryOperation>
OutputIt transform(InputIt1 first1, InputIt1 last1,
                   InputIt2 first2, OutputIt result,
                   BinaryOperation op) {
    while (first1 != last1) {
        *result = op(*first1, *first2);
        ++result;
        ++first1;
        ++first2;
    }
    return result;
}

// Template function for accumulating values
template<typename InputIt, typename T, typename BinaryOperation>
T accumulate(InputIt first, InputIt last, T init, BinaryOperation op) {
    while (first != last) {
        init = op(init, *first);
        ++first;
    }
    return init;
}

// Overload with default addition
template<typename InputIt, typename T>
T accumulate(InputIt first, InputIt last, T init) {
    return TestProject::Algorithms::accumulate(first, last, init, std::plus<T>());
}

// Template function for reducing values (parallel reduction concept)
template<typename InputIt, typename T, typename BinaryOperation>
T reduce(InputIt first, InputIt last, T init, BinaryOperation op) {
    // For this implementation, we'll use sequential reduction
    // In a real parallel implementation, this would use parallel algorithms
    return accumulate(first, last, init, op);
}

// Template function for inner product
template<typename InputIt1, typename InputIt2, typename T>
T inner_product(InputIt1 first1, InputIt1 last1, InputIt2 first2, T init) {
    while (first1 != last1) {
        init = init + (*first1 * *first2);
        ++first1;
        ++first2;
    }
    return init;
}

// Template function for adjacent difference
template<typename InputIt, typename OutputIt, typename BinaryOperation>
OutputIt adjacent_difference(InputIt first, InputIt last, OutputIt result, BinaryOperation op) {
    if (first == last) return result;
    
    typename std::iterator_traits<InputIt>::value_type acc = *first;
    *result = acc;
    ++result;
    
    while (++first != last) {
        typename std::iterator_traits<InputIt>::value_type val = *first;
        *result = op(val, acc);
        acc = std::move(val);
        ++result;
    }
    return result;
}

// Template function for partial sum
template<typename InputIt, typename OutputIt, typename BinaryOperation>
OutputIt partial_sum(InputIt first, InputIt last, OutputIt result, BinaryOperation op) {
    if (first == last) return result;
    
    typename std::iterator_traits<InputIt>::value_type sum = *first;
    *result = sum;
    ++result;
    
    while (++first != last) {
        sum = op(sum, *first);
        *result = sum;
        ++result;
    }
    return result;
}

// Template function for set operations
template<typename InputIt1, typename InputIt2, typename OutputIt, typename Compare>
OutputIt set_union(InputIt1 first1, InputIt1 last1,
                   InputIt2 first2, InputIt2 last2,
                   OutputIt result, Compare comp) {
    while (first1 != last1 && first2 != last2) {
        if (comp(*first1, *first2)) {
            *result = *first1;
            ++first1;
        } else if (comp(*first2, *first1)) {
            *result = *first2;
            ++first2;
        } else {
            *result = *first1;
            ++first1;
            ++first2;
        }
        ++result;
    }
    
    result = std::copy(first1, last1, result);
    return std::copy(first2, last2, result);
}

template<typename InputIt1, typename InputIt2, typename OutputIt, typename Compare>
OutputIt set_intersection(InputIt1 first1, InputIt1 last1,
                          InputIt2 first2, InputIt2 last2,
                          OutputIt result, Compare comp) {
    while (first1 != last1 && first2 != last2) {
        if (comp(*first1, *first2)) {
            ++first1;
        } else if (comp(*first2, *first1)) {
            ++first2;
        } else {
            *result = *first1;
            ++first1;
            ++first2;
            ++result;
        }
    }
    return result;
}

// Template function for heap operations
template<typename RandomIt, typename Compare>
void make_heap(RandomIt first, RandomIt last, Compare comp) {
    std::make_heap(first, last, comp);
}

template<typename RandomIt>
void make_heap(RandomIt first, RandomIt last) {
    using value_type = typename std::iterator_traits<RandomIt>::value_type;
    make_heap(first, last, std::less<value_type>());
}

template<typename RandomIt, typename Compare>
void push_heap(RandomIt first, RandomIt last, Compare comp) {
    std::push_heap(first, last, comp);
}

template<typename RandomIt, typename Compare>
void pop_heap(RandomIt first, RandomIt last, Compare comp) {
    std::pop_heap(first, last, comp);
}

template<typename RandomIt, typename Compare>
void sort_heap(RandomIt first, RandomIt last, Compare comp) {
    std::sort_heap(first, last, comp);
}

// Template function for permutation operations
template<typename BidirIt, typename Compare>
bool next_permutation(BidirIt first, BidirIt last, Compare comp) {
    return std::next_permutation(first, last, comp);
}

template<typename BidirIt>
bool next_permutation(BidirIt first, BidirIt last) {
    using value_type = typename std::iterator_traits<BidirIt>::value_type;
    return next_permutation(first, last, std::less<value_type>());
}

template<typename BidirIt, typename Compare>
bool prev_permutation(BidirIt first, BidirIt last, Compare comp) {
    return std::prev_permutation(first, last, comp);
}

// Template function for sample operation (C++17 style)
template<typename PopulationIt, typename SampleIt, typename Distance, typename UniformRandomBitGenerator>
SampleIt sample(PopulationIt first, PopulationIt last,
                SampleIt out, Distance n,
                UniformRandomBitGenerator&& g) {
    using input_category = typename std::iterator_traits<PopulationIt>::iterator_category;
    using output_category = typename std::iterator_traits<SampleIt>::iterator_category;
    
    // Simplified implementation for demonstration
    if constexpr (std::is_same_v<input_category, std::random_access_iterator_tag>) {
        Distance population_size = std::distance(first, last);
        if (n > population_size) n = population_size;
        
        std::vector<Distance> indices(population_size);
        std::iota(indices.begin(), indices.end(), 0);
        std::shuffle(indices.begin(), indices.end(), g);
        
        for (Distance i = 0; i < n; ++i) {
            *out = *(first + indices[i]);
            ++out;
        }
    } else {
        // For non-random access iterators, use reservoir sampling
        Distance k = 0;
        while (first != last && k < n) {
            *out = *first;
            ++first;
            ++out;
            ++k;
        }
    }
    
    return out;
}

// Template function for sliding window operations
template<typename InputIt, typename OutputIt, typename Size, typename BinaryOperation>
OutputIt sliding_window(InputIt first, InputIt last, OutputIt result, Size window_size, BinaryOperation op) {
    if (first == last || window_size == 0) return result;
    
    std::vector<typename std::iterator_traits<InputIt>::value_type> window;
    window.reserve(window_size);
    
    InputIt current = first;
    
    // Fill initial window
    for (Size i = 0; i < window_size && current != last; ++i) {
        window.push_back(*current);
        ++current;
    }
    
    if (window.size() == window_size) {
        *result = std::accumulate(window.begin(), window.end(), 
                                 typename std::iterator_traits<InputIt>::value_type{}, op);
        ++result;
    }
    
    // Slide the window
    while (current != last) {
        window.erase(window.begin());
        window.push_back(*current);
        
        *result = std::accumulate(window.begin(), window.end(), 
                                 typename std::iterator_traits<InputIt>::value_type{}, op);
        ++result;
        ++current;
    }
    
    return result;
}

// Template function for parallel-style algorithms (conceptual)
template<typename ExecutionPolicy, typename ForwardIt, typename UnaryFunction>
void for_each(ExecutionPolicy&& policy, ForwardIt first, ForwardIt last, UnaryFunction f) {
    // This is a simplified version - real parallel execution would be more complex
    if constexpr (std::is_same_v<std::decay_t<ExecutionPolicy>, std::execution::parallel_policy>) {
        // Simulate parallel execution (in real implementation, this would use threads)
        std::for_each(first, last, f);
    } else {
        std::for_each(first, last, f);
    }
}

} // namespace Algorithms
} // namespace TestProject