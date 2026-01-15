This project aims to develop a lightning-fast, cutting edge actuarial system in Rust that can be used for multiple purposes: pricing, valuation, and projections

We will phase this as follows:

1) liability-only (decremented cashflows) projections for a single policy
2) liability-only projections for multiple policies
3) reserve calculations
4) simple asset modeling in conjunction with our projections, tracking buys, sells, and income
5) asset modeling for fixed and floating rate securities
6) asset portfolio-level analytics
7) multi-scenario model runs
8) embedded decision making (active rebalancing, updated SAA to enhance ALM in future timesteps, etc.)
9) optimization


Some elements (e.g. reserve calculations) may be useful for other purposes, so we'll want to build functions or APIs that we can run in this or other repositories, and to the extent possible we should have tools that demonstrate that our calculations are performing as intended. Ultimately, we will deploy this system to AWS.

the document "Trellis - Reference Rate Calculator 20250926.xlsm" lays out the main assumptions for a static projection, and sheet "Calculation example" can be used as a reference check (if you're able to see formulas). In general, we will want everything done at a monthly timestep. The pricing inforce is in the "Pricing inforce" tab. If you have any other questions, I can provide detailed answers as needed.