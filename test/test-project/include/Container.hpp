#pragma once

#include <vector>
#include <algorithm>
#include <stdexcept>
#include <iterator>
#include <functional>

namespace TestProject {

// Template container class with various useful operations
template<typename T, typename Allocator = std::allocator<T>>
class Container {
private:
    std::vector<T, Allocator> data_;
    size_t capacity_;
    
public:
    // Type definitions for STL compatibility
    using value_type = T;
    using allocator_type = Allocator;
    using size_type = typename std::vector<T, Allocator>::size_type;
    using difference_type = typename std::vector<T, Allocator>::difference_type;
    using reference = typename std::vector<T, Allocator>::reference;
    using const_reference = typename std::vector<T, Allocator>::const_reference;
    using pointer = typename std::vector<T, Allocator>::pointer;
    using const_pointer = typename std::vector<T, Allocator>::const_pointer;
    using iterator = typename std::vector<T, Allocator>::iterator;
    using const_iterator = typename std::vector<T, Allocator>::const_iterator;
    using reverse_iterator = typename std::vector<T, Allocator>::reverse_iterator;
    using const_reverse_iterator = typename std::vector<T, Allocator>::const_reverse_iterator;

    // Nested helper class for statistics
    class Statistics {
    public:
        size_type count;
        T min_value;
        T max_value;
        
        Statistics(size_type c, const T& min_val, const T& max_val) 
            : count(c), min_value(min_val), max_value(max_val) {}
    };

    // Constructors
    Container() : capacity_(0) {}
    
    explicit Container(size_type count, const T& value = T{}) 
        : data_(count, value), capacity_(count) {}
    
    template<typename InputIt>
    Container(InputIt first, InputIt last) 
        : data_(first, last), capacity_(data_.size()) {}
    
    Container(std::initializer_list<T> init) 
        : data_(init), capacity_(data_.size()) {}
    
    // Copy constructor
    Container(const Container& other) 
        : data_(other.data_), capacity_(other.capacity_) {}
    
    // Move constructor
    Container(Container&& other) noexcept 
        : data_(std::move(other.data_)), capacity_(other.capacity_) {
        other.capacity_ = 0;
    }
    
    // Assignment operators
    Container& operator=(const Container& other) {
        if (this != &other) {
            data_ = other.data_;
            capacity_ = other.capacity_;
        }
        return *this;
    }
    
    Container& operator=(Container&& other) noexcept {
        if (this != &other) {
            data_ = std::move(other.data_);
            capacity_ = other.capacity_;
            other.capacity_ = 0;
        }
        return *this;
    }
    
    // Element access
    reference operator[](size_type pos) { return data_[pos]; }
    const_reference operator[](size_type pos) const { return data_[pos]; }
    
    reference at(size_type pos) { return data_.at(pos); }
    const_reference at(size_type pos) const { return data_.at(pos); }
    
    reference front() { return data_.front(); }
    const_reference front() const { return data_.front(); }
    
    reference back() { return data_.back(); }
    const_reference back() const { return data_.back(); }
    
    T* data() noexcept { return data_.data(); }
    const T* data() const noexcept { return data_.data(); }
    
    // Iterators
    iterator begin() noexcept { return data_.begin(); }
    const_iterator begin() const noexcept { return data_.begin(); }
    const_iterator cbegin() const noexcept { return data_.cbegin(); }
    
    iterator end() noexcept { return data_.end(); }
    const_iterator end() const noexcept { return data_.end(); }
    const_iterator cend() const noexcept { return data_.cend(); }
    
    reverse_iterator rbegin() noexcept { return data_.rbegin(); }
    const_reverse_iterator rbegin() const noexcept { return data_.rbegin(); }
    const_reverse_iterator crbegin() const noexcept { return data_.crbegin(); }
    
    reverse_iterator rend() noexcept { return data_.rend(); }
    const_reverse_iterator rend() const noexcept { return data_.rend(); }
    const_reverse_iterator crend() const noexcept { return data_.crend(); }
    
    // Capacity
    bool empty() const noexcept { return data_.empty(); }
    size_type size() const noexcept { return data_.size(); }
    size_type max_size() const noexcept { return data_.max_size(); }
    size_type capacity() const noexcept { return capacity_; }
    
    void reserve(size_type new_cap) {
        data_.reserve(new_cap);
        capacity_ = std::max(capacity_, new_cap);
    }
    
    void shrink_to_fit() {
        data_.shrink_to_fit();
        capacity_ = data_.capacity();
    }
    
    // Modifiers
    void clear() noexcept {
        data_.clear();
    }
    
    iterator insert(const_iterator pos, const T& value) {
        return data_.insert(pos, value);
    }
    
    iterator insert(const_iterator pos, T&& value) {
        return data_.insert(pos, std::move(value));
    }
    
    iterator insert(const_iterator pos, size_type count, const T& value) {
        return data_.insert(pos, count, value);
    }
    
    template<typename InputIt>
    iterator insert(const_iterator pos, InputIt first, InputIt last) {
        return data_.insert(pos, first, last);
    }
    
    iterator insert(const_iterator pos, std::initializer_list<T> ilist) {
        return data_.insert(pos, ilist);
    }
    
    template<typename... Args>
    iterator emplace(const_iterator pos, Args&&... args) {
        return data_.emplace(pos, std::forward<Args>(args)...);
    }
    
    iterator erase(const_iterator pos) {
        return data_.erase(pos);
    }
    
    iterator erase(const_iterator first, const_iterator last) {
        return data_.erase(first, last);
    }
    
    void push_back(const T& value) {
        data_.push_back(value);
    }
    
    void push_back(T&& value) {
        data_.push_back(std::move(value));
    }
    
    template<typename... Args>
    reference emplace_back(Args&&... args) {
        return data_.emplace_back(std::forward<Args>(args)...);
    }
    
    void pop_back() {
        data_.pop_back();
    }
    
    void resize(size_type count) {
        data_.resize(count);
    }
    
    void resize(size_type count, const T& value) {
        data_.resize(count, value);
    }
    
    void swap(Container& other) noexcept {
        data_.swap(other.data_);
        std::swap(capacity_, other.capacity_);
    }
    
    // Advanced operations
    template<typename Predicate>
    size_type count_if(Predicate pred) const {
        return std::count_if(data_.begin(), data_.end(), pred);
    }
    
    template<typename Predicate>
    iterator find_if(Predicate pred) {
        return std::find_if(data_.begin(), data_.end(), pred);
    }
    
    template<typename Predicate>
    const_iterator find_if(Predicate pred) const {
        return std::find_if(data_.begin(), data_.end(), pred);
    }
    
    template<typename Predicate>
    bool all_of(Predicate pred) const {
        return std::all_of(data_.begin(), data_.end(), pred);
    }
    
    template<typename Predicate>
    bool any_of(Predicate pred) const {
        return std::any_of(data_.begin(), data_.end(), pred);
    }
    
    template<typename Predicate>
    bool none_of(Predicate pred) const {
        return std::none_of(data_.begin(), data_.end(), pred);
    }
    
    void sort() {
        std::sort(data_.begin(), data_.end());
    }
    
    template<typename Compare>
    void sort(Compare comp) {
        std::sort(data_.begin(), data_.end(), comp);
    }
    
    void reverse() {
        std::reverse(data_.begin(), data_.end());
    }
    
    template<typename Predicate>
    iterator remove_if(Predicate pred) {
        return std::remove_if(data_.begin(), data_.end(), pred);
    }
    
    iterator unique() {
        return std::unique(data_.begin(), data_.end());
    }
    
    template<typename BinaryPredicate>
    iterator unique(BinaryPredicate pred) {
        return std::unique(data_.begin(), data_.end(), pred);
    }
    
    // Statistical operations (requires T to be comparable)
    Statistics compute_statistics() const {
        if (data_.empty()) {
            throw std::runtime_error("Cannot compute statistics on empty container");
        }
        
        auto minmax = std::minmax_element(data_.begin(), data_.end());
        return Statistics(data_.size(), *minmax.first, *minmax.second);
    }
    
    // Transform operations
    template<typename UnaryOp>
    Container transform(UnaryOp op) const {
        Container result;
        result.reserve(data_.size());
        std::transform(data_.begin(), data_.end(), std::back_inserter(result.data_), op);
        result.capacity_ = result.data_.size();
        return result;
    }
    
    // Comparison operators
    bool operator==(const Container& other) const {
        return data_ == other.data_;
    }
    
    bool operator!=(const Container& other) const {
        return data_ != other.data_;
    }
    
    bool operator<(const Container& other) const {
        return data_ < other.data_;
    }
    
    bool operator<=(const Container& other) const {
        return data_ <= other.data_;
    }
    
    bool operator>(const Container& other) const {
        return data_ > other.data_;
    }
    
    bool operator>=(const Container& other) const {
        return data_ >= other.data_;
    }
};

// Template specialization for bool (bit-packed storage)
template<typename Allocator>
class Container<bool, Allocator> {
private:
    std::vector<bool, Allocator> data_;
    size_t capacity_;
    
public:
    using value_type = bool;
    using allocator_type = Allocator;
    using size_type = typename std::vector<bool, Allocator>::size_type;
    using difference_type = typename std::vector<bool, Allocator>::difference_type;
    using reference = typename std::vector<bool, Allocator>::reference;
    using const_reference = typename std::vector<bool, Allocator>::const_reference;
    
    Container() : capacity_(0) {}
    
    explicit Container(size_type count, bool value = false) 
        : data_(count, value), capacity_(count) {}
    
    void push_back(bool value) {
        data_.push_back(value);
    }
    
    size_type size() const noexcept { return data_.size(); }
    bool empty() const noexcept { return data_.empty(); }
    
    size_type count_true() const {
        return std::count(data_.begin(), data_.end(), true);
    }
    
    size_type count_false() const {
        return std::count(data_.begin(), data_.end(), false);
    }
    
    void flip() {
        for (size_type i = 0; i < data_.size(); ++i) {
            data_[i] = !data_[i];
        }
    }
};

// Free functions for Container
template<typename T, typename Allocator>
void swap(Container<T, Allocator>& lhs, Container<T, Allocator>& rhs) noexcept {
    lhs.swap(rhs);
}

} // namespace TestProject