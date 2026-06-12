## 2025-05-15 - Redundant parsing in O(N^2) loops
**Learning:** The synergy filtering logic was performing expensive symbolic parsing and string extraction inside a nested loop. In scenarios with many synergies, this $O(N^2)$ behavior for parsing was the primary bottleneck.
**Action:** Always pre-calculate and cache expensive metadata in an $O(N)$ pass before entering $O(N^2)$ comparison loops.
