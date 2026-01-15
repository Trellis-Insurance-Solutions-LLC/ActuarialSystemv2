Column Letter	Column Value	First row formula value	Second row formula value
A			
B	Projection month	#N/A	=B11+1
C	Policy year	=FLOOR((B11-1)/12,1)+1	=FLOOR((B12-1)/12,1)+1
D	Month in policy year	#N/A	#N/A
E	Attained age	=$D$4+C11-1	=$D$4+C12-1
F	Baseline mortality	=XLOOKUP(E11,Mortality!$I$9:$I$130,IF($E$4="Female",Mortality!$J$9:$J$130,Mortality!$K$9:$K$130),,-1)	=XLOOKUP(E12,Mortality!$I$9:$I$130,IF($E$4="Female",Mortality!$J$9:$J$130,Mortality!$K$9:$K$130),,-1)
G	Mortality improvement	=XLOOKUP(E11,Mortality!$N$9:$N$130,IF($E$4="Female",Mortality!$O$9:$O$130,Mortality!$P$9:$P$130),,-1)	=XLOOKUP(E12,Mortality!$N$9:$N$130,IF($E$4="Female",Mortality!$O$9:$O$130,Mortality!$P$9:$P$130),,-1)
H	Final mortality	=1-(1-F11*(1-G11)^(2026-2012-1+B11/12))^(1/12)	=1-(1-F12*(1-G12)^(2026-2012-1+B12/12))^(1/12)
I	Surrender charge	=INDEX('Product features '!$C$12:$C$22,MIN(11,C11))	=INDEX('Product features '!$C$12:$C$22,MIN(11,C12))
J	FPW %	=IF(C11=1,0,IF($C$4="Q",MAX('Product features '!$C$8,XLOOKUP(E11,'Non-systematic PWDs'!$D$17:$D$64,'Non-systematic PWDs'!$F$17:$F$64,0,-1)),'Product features '!$C$8))	=IF(C12=1,0,IF($C$4="Q",MAX('Product features '!$C$8,XLOOKUP(E12,'Non-systematic PWDs'!$D$17:$D$64,'Non-systematic PWDs'!$F$17:$F$64,0,-1)),'Product features '!$C$8))
K	GLWB activated	=IF(C11>=$S$4,1,0)	=IF(C12>=$S$4,1,0)
L	Non-systematic PWD rate	=(1-K11)*(1-(1-J11*XLOOKUP(C11,'Non-systematic PWDs'!$D$9:$D$12,'Non-systematic PWDs'!$E$9:$E$12,,-1))^(1/12))	=(1-K12)*(1-(1-J12*XLOOKUP(C12,'Non-systematic PWDs'!$D$9:$D$12,'Non-systematic PWDs'!$E$9:$E$12,,-1))^(1/12))
M	Lapse skew	=IF(C11=11,SWITCH(D11,1,0.4,2,0.3,3,0.2,0.1/9),1/12)	=IF(C12=11,SWITCH(D12,1,0.4,2,0.3,3,0.2,0.1/9),1/12)
N	Premium	=H4	#N/A
O	BOP AV	=N11	=MAX(MAX(0, O11-V11)*(1+U11)*IFERROR((1-T11*P11/O11),0)*(1-H11)*(1-S11)*(1-L11))
P	BOP Benefit base	=N11*(1+O4)	=P11*Y11*(1+IF(AND(D11=12,K11=0),1,0)*W11)
Q	Base component	=XLOOKUP(C11,'Surrender predictive model'!$E$8:$E$29,'Surrender predictive model'!$P$8:$P$29,,-1)+'Surrender predictive model'!$B$15+'Surrender predictive model'!$B$14+'Surrender predictive model'!$B$38*K11	=XLOOKUP(C12,'Surrender predictive model'!$E$8:$E$29,'Surrender predictive model'!$P$8:$P$29,,-1)+'Surrender predictive model'!$B$15+'Surrender predictive model'!$B$14+'Surrender predictive model'!$B$38*K12
R	Dynamic component	=+'Surrender predictive model'!$B$15*(MAX(1,MIN(2,P11/O11))-1)+'Surrender predictive model'!$B$14*(MAX(0.5,MIN(1,P11/O11))-1)+'Surrender predictive model'!$B$38*K11*(MAX(0.5,MIN(1,P11/O11))-1)	=+'Surrender predictive model'!$B$15*(MAX(1,MIN(2,P11/O11))-1)+'Surrender predictive model'!$B$14*(MAX(0.5,MIN(1,P11/O11))-1)+'Surrender predictive model'!$B$38*K12*(MAX(0.5,MIN(1,P11/O11))-1)
S	Final lapse rate	#N/A	=1-(1-IF(O12>0,EXP(SUM(Q12:R12)),0))^M12
T	Rider charge	=IF(K11=1,1.5%,0.5%)*IF(MOD(B11,12)=0,1,0)	=IF(K12=1,1.5%,0.5%)*IF(MOD(B12,12)=0,1,0)
U	Credited rate	=IF($K$4="Fixed",(1+$Z$3*IF(C11>10,0.5,1))^(1/12)-1,IF(D11=12,$Z$4*IF(C11>10,0.5,1),0))	=IF($K$4="Fixed",(1+$Z$3*IF(C12>10,0.5,1))^(1/12)-1,IF(D11=12,$Z$4*IF(C11>10,0.5,1),0))
V	Systematic withdrawal	=IF(C11>=$S$4,$T$4/12,0)*P11	=IF(C12>=$S$4,$T$4/12,0)*P12
W	Rollup rate	=(1+$O$4+$Q$4*MIN(10,C11))/(1+$O$4+$Q$4*MIN(10,C11-1))-1	=(1+$O$4+$Q$4*MIN(10,C12))/(1+$O$4+$Q$4*MIN(10,C12-1))-1
X	AV persistency	=IFERROR((1-H11)*(1-S11)*(1-L11)*(1-T11*P11/O11),0)	=IFERROR((1-H12)*(1-S12)*(1-L12)*(1-T12*P12/O12),0)
Y	BB persistency	=(1-H11)*(1-S11)*(1-L11)	=(1-H12)*(1-S12)*(1-L12)
Z	Lives persistency	=(1-H11)*(1-S11)	=(1-H12)*(1-S12)
AA	Lives	=G4	=AA11*Z11
AB	Pre-decrement AV	=MAX(0, (O11-V11)*(1+U11))	=MAX(0, (O12-V12)*(1+U12))
AC	Mortality	=$AB11*(1-$X11)*H11/($H11+$S11+$L11+IFERROR($T11*$P11/$O11,0))	=$AB12*(1-$X12)*H12/($H12+$S12+$L12+IFERROR($T12*$P12/$O12,0))
AD	Lapse	=$AB11*(1-$X11)*S11/($H11+$S11+$L11+IFERROR($T11*$P11/$O11,0))*(J11+(1-J11)*(1-I11))	=$AB12*(1-$X12)*S12/($H12+$S12+$L12+IFERROR($T12*$P12/$O12,0))*(J12+(1-J12)*(1-I12))
AE	PWD	=$AB11*(1-$X11)*L11/($H11+$S11+$L11+IFERROR($T11*$P11/$O11,0))+V11	=$AB12*(1-$X12)*L12/($H12+$S12+$L12+IFERROR($T12*$P12/$O12,0))+V12
AF	Rider charges	=$AB11*(1-$X11)*IFERROR($T11*$P11/$O11,0)/($H11+$S11+$L11+IFERROR($T11*$P11/$O11,0))	=$AB12*(1-$X12)*IFERROR($T12*$P12/$O12,0)/($H12+$S12+$L12+IFERROR($T12*$P12/$O12,0))
AG	Surrender charges	=$AB11*(1-$X11)*S11/($H11+$S11+$L11+IFERROR($T11*$P11/$O11,0))*(1-J11)*I11	=$AB12*(1-$X12)*S12/($H12+$S12+$L12+IFERROR($T12*$P12/$O12,0))*(1-J12)*I12
AH	Interest credits	=AB11-MAX(0, O11-V11)	=AB12-MAX(0, O12-V12)
AI	EOP AV	=MAX(0, N11+AH11-SUM(AC11:AG11))	=MAX(0, AI11+AH12-SUM(AC12:AG12))
AJ	Expenses	=0.0025/12*AI11	=0.0025/12*AI12
AK	Commission	=XLOOKUP($D$4,$AC$3:$AC$4,$AD$3:$AD$4,,-1)*N11	=XLOOKUP($D$4,$AC$3:$AC$4,$AD$3:$AD$4,,-1)*N12
AL	Chargebacks	=AA11*(1-Z11)/$G$4*$AK$11*IF(C11>1,0,IF(B11>6,0.5,1))	=AA12*(1-Z12)/$G$4*$AK$11*IF(C12>1,0,IF(B12>6,0.5,1))
AM	Bonus comp	=IF(B11=13,O11*XLOOKUP($D$4,$AC$3:$AC$4,$AE$3:$AE$4,,-1),0)	=IF(B12=13,O12*XLOOKUP($D$4,$AC$3:$AC$4,$AE$3:$AE$4,,-1),0)
AN	Total net cashflow	=N11-SUM(AC11:AE11,AJ11:AK11,-AL11,AM11)	=N12-SUM(AC12:AE12,AJ12:AK12,-AL12,AM12)
AO	Net index credit reimbursement	#N/A	=IF($K$4="Fixed",0,O12*IF(D11=12,$Z$4-$X$4*(1+$AA$4),0)*IF(C11>10,0.5,1))
AP	Hedge gains	=IF($K$4="Fixed",0,O11*(1-X11)*$X$4*IF(C11>10, 0.5, 1)*(1+$Y$4-$AA$4)^(D11/12)+AO11)	=IF($K$4="Fixed",0,O12*(1-X12)*$X$4*IF(C11>10, 0.5, 1)*(1+$Y$4-$AA$4)^(D11/12)+AO12)
