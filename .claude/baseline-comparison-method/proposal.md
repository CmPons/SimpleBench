After the recent implementation of benchmark filtering, 5 second warmup and CPU analysis and information we have a better view of the characteristics of a single test. Overall on around 10 runs of the problematic benchmark `bench_vec3_normalize` it was remarkably stable at around a mean of 1.30ms per iteration. However we had one run at `2025-12-11T11-11-27` which was remarkably fast at a mean of `1.20ms`. Due to our comparison method, this was the new baseline, which meant the next run was considered a 'regression' when we went back to 1.30ms per iteration.

Given this data lets research methods for detecting acute regressions in benchmarks. Specifically in scenarios as SimpleBench is designed, a pre-commit check, measured on the same machine. I want to know if anything in that git commit has made the code measurably worse, and fail the commit if so. 

Questions:
- Comparing mean to mean of the last run seems too volatile, what other methods can we use?
- Sometimes code regress acutely, othertimes over a period of months. How can we differentiate the two?
- Given we have all samples for all runs, how can we utilize this data for better statistically rigrorous regression detection?
- Since we have good analysis tools right now can we see if there are any benefits or drawbacks of our monolythic approach running each test combined in the same process, or should we run one test per process? Do we detect any negative interactions between benchmark runs? Given we have bench mark filtering this should be easier to test.
