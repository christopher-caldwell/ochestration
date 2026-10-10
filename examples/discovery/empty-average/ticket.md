# Return no average for an empty list

Calling `average([])` raises `ZeroDivisionError`. Return `None` when the list is empty.
For non-empty finite lists of numbers, preserve the arithmetic mean, including negative numbers
and a mean of zero.

Acceptance: `python3 -m unittest -v` passes all four supplied checks after implementation.

Keep the public function name and call signature. Do not add dependencies, a command-line
interface, or new input-validation behavior. This ticket is limited to the empty-list bug.
