seriatimProjection<-
    inforce %>% 
    # filter(PolicyID <= 1) %>%
  mutate(Horizon=(121 - IssueAge)*12) %>% 
  uncount(Horizon, .id='ProjectionMonth') %>% 
  mutate(CurrentPolicyMonth=1,
         CurrentFundValue=InitialPremium) %>% 
  mutate(PolicyMonth=CurrentPolicyMonth+ProjectionMonth-1,
         PolicyYear=floor((PolicyMonth-1)/12)+1,
         MonthInPolicyYear=PolicyMonth-(PolicyYear-1)*12,
         AttainedAge=IssueAge+PolicyYear-1,
         GLWBBeginAge=pmax(50, IssueAge + WaitPeriod),
           YearsSinceGLWBBegin=AttainedAge - GLWBBeginAge,
         Era=fcase(PolicyYear==SCPeriod+1, 'Shock Year', PolicyYear==1, 'Year 1', default='Other'),
         ProjectionYear=floor((ProjectionMonth-1)/12)+1) %>% 
  filter(AttainedAge<=120) %>% 
  (function(projectionShell) assumptions %>% 
  reduce(left_join,
         .init=projectionShell %>% head(nrow(.)))
  ) %>% 
  mutate_at(vars(SurrenderCharge, MortImprovement, maxPremiumFactor, reinsPremiumRate), ~replace_na(., 0)) %>% 
    mutate(across(Benefit_Base_Bucket, ~if_else(.x %in% c('[200000, 500000)','[500000, Inf)'), '[200000, Inf)', .x))) %>% 
  mutate(BaseLapseRate=predict(
    lapseModel,
    {.} %>% 
      mutate(FS_DurationExposure=1,
             Duration=PolicyYear,
             SurrenderChargePeriod=SCPeriod,
             IncomeStarted=if_else(PolicyYear >= GLWBStartYear, 'Y','N'),
             ITMness=1,
             ChangeInTreasury=0), type='response')) %>% 
  mutate(across(RMDRate, ~replace_na(.x, 0)),
         PWDRate=pmax(FPWPct, RMDRate)*PWDUtilization) %>% 
  mutate_at(vars(LapseSkew), ~replace_na(., 1/12)) %>% 
  mutate_at(vars(PWDRate, FPWPct), ~if_else(PolicyYear==1, 0, .)) %>%
  mutate_at(vars(BaseLapseRate), ~if_else(PolicyMonth==1, 0, .)) %>% 
  mutate(across(BEMortality, ~replace_na(.*approx(c(60, 90), c(.6, 1), AttainedAge, rule=2)$y*(1-MortImprovement)^(MIStart_Proj+ProjectionMonth/12), 1)),
         across(StatMortality, ~replace_na(.*(1-MortImprovement)^(MIStart_Val+ProjectionMonth/12), 1))) %>% 
  mutate_at(vars(BEMortality, StatMortality), ~1-(1-pmin(., .999))^(1/12)) %>% 
    mutate(across(c(FixedRate, BaseOptionBudget), ~if_else(PolicyYear > SCPeriod, .5, 1)*.x)) %>% 
  mutate(CreditedRate=if_else(CreditingStrategy=='Fixed', (1+FixedRate)^(1/12)-1, if_else(PolicyMonth %% 12 ==1, 1, 0)*if_else(PolicyYear>1, 1, 0)*replace_na(FastGroupedLag(PolicyID, BaseOptionBudget), 0)*(1+equityKicker)),
         StatCreditedRate=if_else(PolicyYear<=1, if_else(CreditingStrategy=='Fixed', FixedRate, BaseOptionBudget*0+MGIR), MGIR)) %>% 
  mutate(across(FPWPct, ~if_else(SurrenderCharge==0, 1, pmax(.x, RMDRate)))) %>% 
  left_join(GLWBPVPayments) %>% 
  left_join(wbFactors, by=join_by(closest(GLWBBeginAge>=Age))) %>%
  left_join(wbFactors %>% rename(CurrentFactor=GLWBPayoutFactor), by=join_by(AttainedAge==Age)) %>%
  mutate(across(CurrentFactor, ~replace_na(., 0))) %>% 
  mutate(
    NonSystematicWDRate=1-(1-if_else(AttainedAge<GLWBBeginAge, PWDRate*if_else(GLWBStartYear<100, 1, 1), 0))^(1/12),
    RiderCharge=if_else(PolicyYear<=WaitPeriod, .005, .015)) %>%  
  mutate(across(Rollup, ~if_else(RollupType=='Simple', (InitialBB/InitialPremium+.x*PolicyYear)/(InitialBB/InitialPremium+.x*(PolicyYear-1))-1, .x))) %>% 
  bind_cols(with(., GLWBAV(PolicyID, CurrentFundValue, 
                           InitialBB, BEMortality, 
                           BaseLapseRate,
                           CreditedRate, 
                           Rollup*if_else(MonthInPolicyYear==12, 1, 0)*if_else(PolicyYear<=pmin(10, WaitPeriod), 1, 0),
                           if_else(MonthInPolicyYear == 12, RiderCharge, 0), 
                           if_else(AttainedAge>=GLWBBeginAge, GLWBPayoutFactor, 0), 
                           NonSystematicWDRate, 
                           PVWBPayments*0, 
                           if_else(PolicyYear==1, .005, .01)*0, LapseSkew)) %>% 
              set_names('BOPAV','BOPGLWBBase','GLWBPayment', 'finalLapseRate', 'finalRiderCharge', 'finalITMness')) %>% 
  mutate_at(vars(valRate, MGIR, StatCreditedRate), ~(1+.)^(1/12)-1) %>% 
  mutate(persistency_Lives=(1-BEMortality)*(1-finalLapseRate),
         Lives=InitialPols*FastGroupedPersistency(PolicyID, persistency_Lives),
         across(Lives, ~if_else(is.na(.x), InitialPols, .x)),
         across(c(finalRiderCharge, BOPAV), ~if_else(is.nan(.) | is.infinite(.), 0, .)),
         PreDecrementAV=pmax(0, (BOPAV - GLWBPayment)*(1+CreditedRate)),
         PWDRate=if_else(GLWBPayment>0, 0, NonSystematicWDRate),
         persistency_AV=(1-BEMortality)*(1-finalLapseRate)*(1-PWDRate)*pmax(1-finalRiderCharge, 0)) %>% 
  # mutate(across(FPWPct, ~0)) %>% 
  mutate(Decrements_Mort=(PreDecrementAV)*(1-persistency_AV)*BEMortality/(BEMortality + PWDRate + finalLapseRate + finalRiderCharge),
         Decrements_PWD=(PreDecrementAV)*(1-persistency_AV)*PWDRate/(BEMortality + PWDRate + finalLapseRate + finalRiderCharge) + 
           GLWBPayment/if_else(GLWBPayment>BOPAV, 1, 1),
         Decrements_Lapse=(PreDecrementAV)*(1-persistency_AV)*finalLapseRate/
           (BEMortality + PWDRate + finalLapseRate + finalRiderCharge)*(FPWPct + (1-FPWPct)*(1-SurrenderCharge)),
         InterestCredited=PreDecrementAV-pmax(0, BOPAV - GLWBPayment),
         RiderCharges=(PreDecrementAV)*(1-persistency_AV)*finalRiderCharge/(BEMortality + PWDRate + finalLapseRate + finalRiderCharge),
         SurrenderCharges=(PreDecrementAV)*(1-persistency_AV)*finalLapseRate/
           (BEMortality + PWDRate + finalLapseRate + finalRiderCharge)*(1-FPWPct)*SurrenderCharge,
         GLWBClaims=pmax(0, GLWBPayment - BOPAV)) %>% 
    
    ## Below chunk is to proxy death benefits in activated GLWB CARVM stream
  map_dfc(rev) %>%
  mutate(StatPVDB=PVBenefits(PolicyID, StatMortality, 1-StatMortality, 1/(1+valRate))) %>%
  map_dfc(rev) %>%
  mutate(StatPersistency=FastGroupedPersistency(PolicyID, 1-StatMortality)) %>%
  group_by(PolicyID) %>%
  mutate(TermProjYear=pmin(round(1/(GLWBPayoutFactor/12*BOPGLWBBase/BOPAV)/2, 0), n() - row_number() + 1)) %>%
  mutate(Ax=StatPVDB - nAhead(StatPVDB*StatPersistency, TermProjYear)/(1+valRate)^round(1/(GLWBPayoutFactor/12)/2)/StatPersistency) %>%
  # mutate(Ax = 0) %>%  ## Use this to shut off proxy death benefits
  ungroup() %>%
    ## End proxy death benefit chunk
  map_dfc(rev) %>% 
  mutate(BaseReserveFactors=StatResFactors(PolicyID, PolicyYear, SCPeriod, StatCreditedRate, valRate, StatMortality,
                                       fcase(
                                         SurrenderCharge==0, 1,
                                         MonthInPolicyYear==1, FPWPct,
                                         ProjectionMonth>0, 0),
                                       rep(0, nrow(.)), rep(0, nrow(.)), 
                                       SurrenderCharge)[[1]]) %>%
  mutate(GLWBReserveFactors=StatResFactors(PolicyID, PolicyYear, SCPeriod, 
                                           if_else(PolicyYear<=10 & MonthInPolicyYear==12, Rollup, 0), 
                                           valRate, StatMortality,
                                       rep(0, nrow(.)),
                                       StatPVPayments*CurrentFactor/12, 1-BOPAV/BOPGLWBBase*0, rep(1, nrow(.)))[[1]],
         CARVMStartYear=StatResFactors(PolicyID, PolicyYear, SCPeriod, 
                                           if_else(PolicyYear<=10 & MonthInPolicyYear==12, Rollup, 0), 
                                           valRate, StatMortality,
                                       rep(0, nrow(.)),
                                       StatPVPayments*CurrentFactor/12, 1-BOPAV/BOPGLWBBase*0, rep(1, nrow(.)))[[2]]) %>%
  left_join({.} %>% 
              group_by(PolicyID, IssueAge) %>% 
              summarise(across(CARVMStartYear, min)) %>% 
              ungroup() %>% 
              mutate(CARVMAge=IssueAge + CARVMStartYear - 1) %>% 
              left_join(wbFactors, by=c('CARVMAge'='Age')) %>% 
              select(PolicyID, CARVMFactor=GLWBPayoutFactor)) %>%     
  mutate(GLWBReserve=GLWBReserveFactors*BOPGLWBBase,
         BaseReserve=BaseReserveFactors*BOPAV,
         BOPStatReserve=pmax(if_else(AttainedAge>=GLWBBeginAge, 
                                     GLWBPayoutFactor/12*StatPVPayments*BOPGLWBBase + Ax*BOPAV, 
                                     GLWBReserve), BaseReserve)) %>% 
    group_by(PolicyID) %>% 
    mutate(
         EOPStatReserve=lag(BOPStatReserve),
         EOPStatReserve=fifelse(is.na(EOPStatReserve), BOPStatReserve, EOPStatReserve),
         EOPCSV=lag(BOPAV)*(FPWPct + (1-FPWPct)*(1-SurrenderCharge)),
         EOPAV=replace_na(lag(BOPAV),0),
         EOPCSV=fifelse(is.na(EOPCSV), BOPAV*(FPWPct + (1-FPWPct)*(1-SurrenderCharge)), EOPCSV),
         BOPCSV=BOPAV*(FPWPct + (1-FPWPct)*(1-SurrenderCharge))) %>%
    ungroup() %>% 
  map_dfc(rev) %>% 
    # group_by(PolicyID) %>% 
  mutate(
    across(CommissionPct, ~if_else(SCPeriod==15, .x + .01, .x)),
    Commission=case_when(
    PolicyMonth==1 ~ CommissionPct*InitialPremium, 
    PolicyMonth==13 ~ .005/if_else(SCPeriod==10, .07, .08)*(CommissionPct - OverridePct)*BOPAV,
    TRUE ~0),
    Chargebacks=FastGroupedPersistency(PolicyID, persistency_Lives)*(1-persistency_Lives)*CommissionPct*InitialPremium*fcase(PolicyYear>1, 0, PolicyMonth>6, .5, default=1),
    Override=FastGroupedPersistency(PolicyID, persistency_Lives)*(1-persistency_Lives)*OverridePct*InitialPremium*fcase(PolicyYear>1, 0, PolicyMonth>6, .5, default=1),
         # MaintenanceExpenses=.002/12*BOPStatReserve,
    MaintenanceExpenses=.0025/12*EOPAV,
    
         AcquisitionExpenses=0,
         RateHoldExpense=.0003/12*InitialPremium*if_else(PolicyYear==1, 1, 0)*0,
         HedgingExpense=.0005*BOPAV/12*0,
         Premium=if_else(ProjectionMonth==1, 1, 0)*InitialPremium,
         TotalNetCashflow=Premium - Decrements_Lapse - Decrements_PWD - Decrements_Mort - Commission + Chargebacks - AcquisitionExpenses - MaintenanceExpenses - RateHoldExpense -
           HedgingExpense)


summaryCFs_direct<-seriatimProjection %>% 
  mutate(EarnedRate=.0675,
         c1=.02) %>% 
  # mutate(HedgeGains=if_else(CreditingStrategy=='Fixed', 0, BOPAV*equityKicker/12*BaseOptionBudget),
  #        EOPHedgeAssets=EOPAV*BaseOptionBudget*if_else(CreditingStrategy=='Fixed', 0, 1),
  #        NetIndexCreditReimbursement=0) %>% 
  mutate(NetIndexCreditReimbursement=if_else(CreditingStrategy=='Fixed', 0, BOPAV*pmax(0, CreditedRate - replace_na(FastGroupedLag(PolicyID, BaseOptionBudget), 0)*1.05))) %>%
  mutate(HedgeGains=BOPAV*(1-persistency_AV)*if_else(CreditingStrategy=='Fixed', 0, BaseOptionBudget)*(1+equityKicker - .05)^(MonthInPolicyYear/12) + NetIndexCreditReimbursement,
         EOPHedgeAssets=0) %>%
  mutate(TaxReserve=if_else(QualStatus=='Q', EOPStatReserve, pmax(EOPCSV, EOPStatReserve*.9281)),
         # TaxReserve=pmax(EOPCSV, EOPStatReserve*.9281),
         Expenses=AcquisitionExpenses+MaintenanceExpenses+RateHoldExpense+HedgingExpense-Chargebacks,
         c3=if_else(SurrenderCharge>0, .005, .02),
         c4=if_else(ProjectionMonth<=12, .02, 0),
         reqCapital=4*(EOPStatReserve*(c1+c3)+c4*InitialPremium)/(1-4*c1),
         dacTaxImpact=case_when(
           PolicyMonth==1~290/300,
           PolicyYear==1~0,
           PolicyYear<16~-20/300,
           PolicyYear==16~-10/300,
           TRUE~0
         )*if_else(QualStatus=='N', 1, 0)*InitialPremium*.0209/if_else(PolicyYear==1, 1, 12)
  ) %>% 
  group_by(ProjectionMonth, ProjectionYear=floor((ProjectionMonth-1)/12)+1, EarnedRate, c1, c3, c4) %>% 
  summarise(across(c(InitialPremium, Lives, BOPAV, PreDecrementAV, Expenses, 
                    AcquisitionExpenses, MaintenanceExpenses, RateHoldExpense, HedgingExpense, Chargebacks, EOPAV,
                    Commission, Decrements_Lapse, InterestCredited, Override,
                    Decrements_Mort, Decrements_PWD, BOPStatReserve, EOPStatReserve, EOPCSV, 
                    TaxReserve, reqCapital, dacTaxImpact, HedgeGains, EOPHedgeAssets,
                    NetIndexCreditReimbursement), sum),
            across(CreditedRate, ~mean((1+.)^12-1))) %>% 
  ungroup() %>% 
  mutate(across(EarnedRate, ~(1+.)^(1/12)-1)) %>% 
  mutate(TotalNetCashflow=if_else(ProjectionMonth==1, 1, 0)*InitialPremium - (Decrements_Lapse + Decrements_PWD + Decrements_Mort + Expenses + Commission)) %>% 
  mutate(TotalAssets=EOPStatReserve+reqCapital,
         InvestmentIncome=EarnedRate*if_else(is.na(lag(TotalAssets - EOPHedgeAssets)), TotalNetCashflow, lag(TotalAssets - EOPHedgeAssets)) + HedgeGains,
         ChangeInStatReserve=EOPStatReserve-replace_na(lag(EOPStatReserve), 0),
         ChangeInTaxReserve=TaxReserve-replace_na(lag(TaxReserve), 0),
         ChangeInCapital=reqCapital-replace_na(lag(reqCapital), 0),
         PreTaxNetIncome=InvestmentIncome + TotalNetCashflow-ChangeInStatReserve,
         Taxes=-.21*(PreTaxNetIncome+ChangeInStatReserve-ChangeInTaxReserve+dacTaxImpact),
         StatNetIncome=PreTaxNetIncome+Taxes) %>% 
  mutate(FreeCashflow=StatNetIncome-ChangeInCapital)           