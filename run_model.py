PRIMARY_ASSET = 'USDC' # Safe asset
SECONDARY_ASSET = 'ATOM'  # Risky asset that would be stacked
TRADING_PAIR_NAME = f"{SECONDARY_ASSET}_{PRIMARY_ASSET}"

USE_PARQUET_FILES = True # Use *.parquet or Binance CSV files?

MINUTES_PER_MODEL_TICK = 5 # minutes
MODEL_TICKS_PER_YEAR = 365.25 * 24 * 60 / MINUTES_PER_MODEL_TICK

MONTHS_TO_MODEL = 6
INTERVALS_TO_MODEL = int(MONTHS_TO_MODEL * MODEL_TICKS_PER_YEAR / 12)

# Capital available
INITIAL_BALANCE_PRIMARY = 1_000_000 # Primary asset

# Target hedge margin fractions
TARGET_MARGIN_FRACTION = 0.25
LOW_MARGIN_FRACTION = 0.1
HIGH_MARGIN_FRACTION = 0.5
TARGET_COLLATERAL_FRACTION = TARGET_MARGIN_FRACTION / (1 + TARGET_MARGIN_FRACTION)

# Amount of time that is required to fully recover after liquidation (assuming manual recovery procedure)
LIQUIDATION_RECOVERY_PERIOD_MINUTES = 8 * 60
LIQUIDATION_RECOVERY_PERIOD_MODEL_TICKS = LIQUIDATION_RECOVERY_PERIOD_MINUTES / MINUTES_PER_MODEL_TICK

# Staking rewards, annualized
STACKING_REWARD_PERCENT = 8.5 # TODO compound/reinvestment
STACKING_REWARD_PER_MODEL_TICK_PERCENT = STACKING_REWARD_PERCENT / MODEL_TICKS_PER_YEAR

# Funding rewards, annualized
FUNDING_FEE_PERCENT = 9.0 # TODO compound/reinvestment
FUNDING_FEE_PER_MODEL_TICK_PERCENT = FUNDING_FEE_PERCENT / MODEL_TICKS_PER_YEAR

# Percentage of volume we can trade without affecting market too much
ACCESSIBLE_MARKET_VOLUME_FRACTION = 5

# Transaction costs ---------------------------------------
SLIPPAGE_PERCENT = 0.1
EXCHANGE_FEE_PERCENT = 0.05

# Misc ----------------------------------------------------
DATADIR = './data/'
BINANCE_DATADIR = f'{DATADIR}/test/'
PARQUETS_DATADIR = DATADIR
CSV_COLUMN_NAMES = ['open_time', 'open_price', 'high_price', 'low_price', 'close_price', 'trade_volume_secondary', 'close_time',
                    'quote_asset_volume', 'number_of_trades', 'taker_buy_base_asset_volume',
                    'taker_buy_quote_asset_volume','ignore']




def run_model(df, do_logging=False):
    """
    Parameters:
        df : dataframe with data for processing
    Returns:
        A dict with model results
    """
    price = df['close_price'][0]
    assert price > 0

    # Initial stake and hedge
    hedge_collateral_primary = INITIAL_BALANCE_PRIMARY * TARGET_COLLATERAL_FRACTION
    stake_primary = -INITIAL_BALANCE_PRIMARY + hedge_collateral_primary
    stake_secondary = -stake_primary / price
    hedge_primary = -stake_primary
    hedge_secondary = -stake_secondary

    result_df = df.copy()
    result_df.insert(2, 'action', pd.Series([[], None], dtype=object))

    last_liquidation_completion_tick_nr = -1 # last model tick number when liquidation process ended/will end
    tick_nr = 0

    for idx, row in result_df.iterrows():
        tick_nr += 1
        price = row.close_price
        assert price > 0
        assert row.trade_volume_secondary >= 0

        # Sanity checks
        assert stake_primary <= 0
        assert stake_secondary >= 0
        assert hedge_primary >= 0
        assert hedge_secondary <= 0
        assert hedge_collateral_primary >= 0
        assert stake_primary * stake_secondary <= 0
        assert hedge_primary * hedge_secondary <= 0

        # Assume hedge price always match spot market
        hedge_price = price
        if hedge_secondary != 0:
            hedge_entry_price = -hedge_primary / hedge_secondary
        else:
            hedge_entry_price = None
        assert hedge_entry_price is None or hedge_entry_price > 0

        action_taken = None
        stake_primary_change = 0
        stake_secondary_change = 0
        hedge_collateral_primary_change = 0
        hedge_primary_change = 0
        hedge_secondary_change = 0

        stake_realized_pnl_primary = 0
        hedge_realized_pnl_primary = 0
        hedge_revaluation_realized_pnl_primary = 0
        stake_unrealized_pnl_primary_before_tick = stake_secondary * price + stake_primary
        hedge_unrealized_pnl_primary_before_tick = hedge_secondary * hedge_price + hedge_primary
        hedge_total_value_primary_before_tick = hedge_collateral_primary + hedge_unrealized_pnl_primary_before_tick
        hedge_total_notional_primary_before_tick = abs(hedge_secondary * hedge_price)

        if hedge_total_notional_primary_before_tick != 0:
            hedge_margin_fraction_before_tick = hedge_total_value_primary_before_tick / hedge_total_notional_primary_before_tick
        else:
            hedge_margin_fraction_before_tick = None

        if hedge_margin_fraction_before_tick is not None and hedge_margin_fraction_before_tick != 0:
            hedge_leverage_before_tick = 1 / hedge_margin_fraction_before_tick
        else:
            hedge_leverage_before_tick = None

        if hedge_entry_price is not None:
            if hedge_secondary <= 0: # Hedge Short
                liquidation_price = hedge_entry_price / (1 + TARGET_MARGIN_FRACTION)
            else: # Hedge Long
                liquidation_price = hedge_entry_price / (1 - TARGET_MARGIN_FRACTION)
        else:
            liquidation_price = None

        if tick_nr > last_liquidation_completion_tick_nr:
            # Normal mode
            if hedge_total_value_primary_before_tick <= 0:
                # Liquidation triggered
                action_taken = "Liquidation triggered"
                hedge_secondary_change = -hedge_secondary
                hedge_primary_change = -hedge_primary
                hedge_collateral_primary_change = -hedge_collateral_primary
                hedge_realized_pnl_primary = -hedge_collateral_primary
                last_liquidation_completion_tick_nr = tick_nr + LIQUIDATION_RECOVERY_PERIOD_MODEL_TICKS

                if do_logging:
                    print(f"Liquidation triggered at {row.time}")
            else:
                if hedge_margin_fraction_before_tick < LOW_MARGIN_FRACTION or hedge_margin_fraction_before_tick > HIGH_MARGIN_FRACTION:
                    action_taken = "Hedge readjustment"
                    # Low/High margin fraction. We have to unstake/stake some secondary asset and use it to increase/decrease hedge collateral

                    # Check if hedge revaluation is required (close and open position)
                    # Save old hedge primary value to calculate changes
                    hedge_primary_revaluation_change = 0
                    if hedge_secondary <= 0: # Hedge Short
                        print('--- hedge_secondary <= 0 ---')
                        if hedge_price <= liquidation_price:
                            action_taken = "Hedge revaluation & readjustment"
                            # Trigger P&L realization/hedge revaluation
                            # That is equivalent to closing and reopening hedge position
                            hedge_revaluation_realized_pnl_primary = hedge_primary + hedge_secondary * hedge_price
                            hedge_primary_revaluation_change = -hedge_secondary * hedge_price - hedge_primary
                            # if do_logging:
                            #   print(f"Hedge revaluation {row.time}")
                    else: # Hedge Long
                        assert False
                        assert hedge_price < liquidation_price
                        if hedge_price >= liquidation_price:
                            action_taken = "Long hedge revaluation & readjustment"
                            # Trigger P&L realization/hedge revaluation
                            # That is equivalent to closing and reopening hedge position
                            hedge_revaluation_realized_pnl_primary = hedge_primary + hedge_secondary * hedge_price
                            hedge_primary_revaluation_change = -hedge_secondary * hedge_price - hedge_primary
                        print(f"Error at {row.time}: Hedge long\nHedge: {hedge_primary} {PRIMARY_ASSET}/{hedge_secondary} {SECONDARY_ASSET}\nStake: {stake_primary} {PRIMARY_ASSET}/{stake_secondary} {SECONDARY_ASSET}")
                        assert 1 == 0 # This should not happen

                    # Collateral change required to get to Target margin fraction
                    # This assumes hedge is short
                    hedge_entry_price = -(hedge_primary + hedge_primary_revaluation_change)/ hedge_secondary # We have to calculate it once again taking into account hedge revaluation
                    required_collateral_change_secondary = (hedge_collateral_primary + hedge_revaluation_realized_pnl_primary + hedge_secondary * (hedge_price * (1 + TARGET_MARGIN_FRACTION) - hedge_entry_price)) / \
                                                           (hedge_price * (1 + TARGET_MARGIN_FRACTION))

                    # Set hedge collateral change limiting it by max change by accessible fraction of the overall trade volume
                    hedge_collateral_secondary_change = math.copysign(min(
                            row.trade_volume_secondary * ACCESSIBLE_MARKET_VOLUME_FRACTION,
                            abs(required_collateral_change_secondary)
                        ), required_collateral_change_secondary)

                    # >>>>>>>>> Get realized P&L for hedge position
                    # Set hedge change to same value as collateral change
                    hedge_secondary_change = -hedge_collateral_secondary_change
                    hedge_primary_position_delta = -hedge_secondary_change * hedge_price
                    assert hedge_secondary_change * hedge_primary_position_delta <= 0

                    # Collapse hedge position
                    if hedge_secondary_change * hedge_secondary < 0:
                        hedge_to_collapse_secondary = math.copysign(min(abs(hedge_secondary_change), abs(hedge_secondary)), hedge_secondary_change)
                    else:
                        hedge_to_collapse_secondary = 0

                    if hedge_secondary_change == 0:
                        hedge_change_to_collapse_primary = hedge_primary_position_delta
                    else:
                        hedge_change_to_collapse_primary = hedge_primary_position_delta * \
                                                                (hedge_to_collapse_secondary / hedge_secondary_change)
                    # Collapse delta hedge position
                    if hedge_secondary == 0:
                        delta_hedge_to_collapse_primary = (hedge_primary + hedge_primary_revaluation_change)
                    else:
                        delta_hedge_to_collapse_primary = (hedge_primary + hedge_primary_revaluation_change) * (-hedge_to_collapse_secondary / hedge_secondary)

                    # P&L is always denominated in primary asset
                    hedge_realized_pnl_primary = hedge_change_to_collapse_primary + delta_hedge_to_collapse_primary
                    # Realized P&L from hedge has to be taken into account as well
                    hedge_collateral_primary_change = -hedge_collateral_secondary_change * hedge_price + hedge_revaluation_realized_pnl_primary + hedge_realized_pnl_primary

                    # New position
                    if hedge_secondary_change * hedge_secondary > 0:
                        # Positions with same direction
                        new_hedge_primary = hedge_primary_position_delta + (hedge_primary + hedge_primary_revaluation_change)
                    else:
                        # Positions with opposing directions
                        if abs(hedge_secondary_change) > abs(hedge_secondary):
                            new_hedge_primary = hedge_primary_position_delta - hedge_change_to_collapse_primary
                        else:
                            new_hedge_primary = hedge_primary - delta_hedge_to_collapse_primary
                    hedge_primary_change = new_hedge_primary - hedge_primary

                    # >>>>>>>>> Get realized P&L for spot position
                    # Set stake change to same value as collateral change but with negative sign
                    stake_secondary_change = hedge_collateral_secondary_change
                    stake_primary_position_delta = -stake_secondary_change * price
                    assert stake_secondary_change * stake_primary_position_delta <= 0

                    # Collapse stake position
                    if stake_secondary_change * stake_secondary < 0:
                        stake_change_to_collapse_secondary = math.copysign(min(abs(stake_secondary_change), abs(stake_secondary)), stake_secondary_change)
                    else:
                        stake_change_to_collapse_secondary = 0

                    if stake_secondary_change == 0:
                        stake_change_to_collapse_primary = stake_primary_position_delta
                    else:
                        stake_change_to_collapse_primary = stake_primary_position_delta * \
                                                                (stake_change_to_collapse_secondary / stake_secondary_change)
                    # Collapse delta stake position
                    if stake_secondary == 0:
                        delta_stake_to_collapse_primary = stake_primary
                    else:
                        delta_stake_to_collapse_primary = stake_primary * (-stake_change_to_collapse_secondary / stake_secondary)

                    # P&L always is denominated in primary asset
                    stake_realized_pnl_primary = stake_change_to_collapse_primary + delta_stake_to_collapse_primary

                    # New position
                    if stake_secondary_change * stake_secondary > 0:
                        # Positions with same direction
                        new_stake_primary = stake_primary_position_delta + stake_primary
                    else:
                        # Positions with opposing directions
                        if abs(stake_secondary_change) > abs(stake_secondary):
                            new_stake_primary = stake_primary_position_delta - stake_change_to_collapse_primary
                        else:
                            new_stake_primary = stake_primary - delta_stake_to_collapse_primary
                    stake_primary_change = new_stake_primary - stake_primary

                    if do_logging:
                        if hedge_margin_fraction_before_tick < LOW_MARGIN_FRACTION:
                            print(f"Low hedge margin fraction {row.time}")
                        elif hedge_margin_fraction_before_tick > HIGH_MARGIN_FRACTION:
                            print(f"High hedge margin fraction {row.time}")
        elif tick_nr == last_liquidation_completion_tick_nr:
            # Liquidation processing completed
            # Restore hedge margin using part of staked assets
            action_taken = 'Liquidation recovery'

            stake_secondary_change = -stake_secondary * TARGET_COLLATERAL_FRACTION
            stake_primary_change = -stake_primary * TARGET_COLLATERAL_FRACTION
            stake_realized_pnl_primary = -(price + stake_primary / stake_secondary) * stake_secondary_change

            hedge_collateral_primary_change = -stake_secondary_change * price

            # Hedge is 0 after liquidation, no pnl here
            hedge_secondary_change = -hedge_secondary - stake_secondary - stake_secondary_change
            hedge_primary_change = -hedge_secondary_change * hedge_price
            hedge_realized_pnl_primary = 0

            if do_logging:
                if do_logging:
                    print(f"Liquidation completed {row.time}")
        elif tick_nr < last_liquidation_completion_tick_nr:
            # We have just been liquidated. Do nothing until liquidation recovery period ends
            action_taken = 'Wait for liquidation recovery'

        realized_pnl_primary = stake_realized_pnl_primary + hedge_realized_pnl_primary

        result_df.at[idx, 'action'] = action_taken

        # Values before changes
        result_df.at[idx, 'stake_primary_before'] = stake_primary
        result_df.at[idx, 'stake_secondary_before'] = stake_secondary
        result_df.at[idx, 'hedge_primary_before'] = hedge_primary
        result_df.at[idx, 'hedge_secondary_before'] = hedge_secondary
        result_df.at[idx, 'hedge_collateral_primary_before'] = hedge_collateral_primary

        # Position changes
        stake_primary += stake_primary_change
        stake_secondary += stake_secondary_change
        hedge_primary += hedge_primary_change
        hedge_secondary += hedge_secondary_change
        hedge_collateral_primary += hedge_collateral_primary_change

        # stake_unrealized_pnl_primary_after_tick = stake_secondary * price + stake_primary
        hedge_unrealized_pnl_primary_after_tick = hedge_secondary * hedge_price + hedge_primary
        hedge_total_value_primary_after_tick = hedge_collateral_primary + hedge_unrealized_pnl_primary_after_tick
        hedge_total_notional_primary_after_tick = abs(hedge_secondary * hedge_price)

        if hedge_total_notional_primary_after_tick != 0:
            hedge_margin_fraction_after_tick = hedge_total_value_primary_after_tick / hedge_total_notional_primary_after_tick
        else:
            hedge_margin_fraction_after_tick = None

        if hedge_margin_fraction_after_tick is not None and hedge_margin_fraction_after_tick != 0:
            hedge_leverage_after_tick = 1 / hedge_margin_fraction_after_tick
        else:
            hedge_leverage_after_tick = None

        result_df.at[idx, 'hedge_margin_fraction_before'] = liquidation_price

        result_df.at[idx, 'hedge_margin_fraction_before'] = hedge_margin_fraction_before_tick
        result_df.at[idx, 'hedge_margin_fraction_after'] = hedge_margin_fraction_after_tick
        result_df.at[idx, 'hedge_leverage_before'] = hedge_leverage_before_tick
        result_df.at[idx, 'hedge_leverage_after'] = hedge_leverage_after_tick
        result_df.at[idx, 'total_hedge_value_primary_before'] = hedge_total_value_primary_before_tick

        result_df.at[idx, 'stake_unrealized_pnl_primary_before'] = stake_unrealized_pnl_primary_before_tick
        result_df.at[idx, 'hedge_unrealized_pnl_primary_before'] = hedge_unrealized_pnl_primary_before_tick

        result_df.at[idx, 'stake_primary_change'] = stake_primary_change
        result_df.at[idx, 'stake_secondary_change'] = stake_secondary_change
        result_df.at[idx, 'hedge_primary_change'] = hedge_primary_change
        result_df.at[idx, 'hedge_secondary_change'] = hedge_secondary_change
        result_df.at[idx, 'hedge_collateral_primary_change'] = hedge_collateral_primary_change

        result_df.at[idx, 'hedge_revaluation_realized_pnl_primary'] = hedge_revaluation_realized_pnl_primary
        result_df.at[idx, 'stake_realized_pnl_primary'] = stake_realized_pnl_primary
        result_df.at[idx, 'hedge_realized_pnl_primary'] = hedge_realized_pnl_primary
        result_df.at[idx, 'realized_pnl_primary'] = realized_pnl_primary
        result_df.at[idx, 'hedge_revaluation_realized_pnl_primary'] = hedge_revaluation_realized_pnl_primary

        # Values after changes
        result_df.at[idx, 'stake_primary'] = stake_primary
        result_df.at[idx, 'stake_secondary'] = stake_secondary
        result_df.at[idx, 'hedge_primary'] = hedge_primary
        result_df.at[idx, 'hedge_secondary'] = hedge_secondary
        result_df.at[idx, 'hedge_collateral_primary'] = hedge_collateral_primary

        # Total hedge account value at each model tick
        hedge_unrealized_pnl_primary = hedge_primary + hedge_secondary * hedge_price
        hedge_total_value_primary = hedge_collateral_primary + hedge_unrealized_pnl_primary
        result_df.at[idx, 'total_hedge_value_primary'] = hedge_total_value_primary


    result_df['staking_reward_secondary'] = list(
        it.accumulate(result_df['stake_secondary'],
                      initial=0,
                      func=lambda p, c: p + c * STACKING_REWARD_PER_MODEL_TICK_PERCENT / 100))[1:]
    result_df['funding_fee_primary'] = list(
        it.accumulate(result_df['stake_secondary'] * df['close_price'],
                      initial=0,
                      func=lambda p, sc: p + sc * FUNDING_FEE_PER_MODEL_TICK_PERCENT / 100))[1:]

    result_df['hedge_unrealized_pnl'] = result_df['hedge_primary'] + result_df['hedge_secondary'] * result_df['close_price']
    result_df['hedge_collateral_primary'] =result_df['hedge_collateral_primary']

    result_df['stake_unrealized_pnl'] = \
        result_df['stake_primary'] + result_df['stake_secondary'] * result_df['close_price']

    result_df['hedge_plus_stake_unrealized_pnl'] = \
        result_df['hedge_primary'] + result_df['hedge_secondary'] * result_df['close_price'] + \
        result_df['stake_primary'] + result_df['stake_secondary'] * result_df['close_price']

    result_df['total_unhedged_secondary'] = result_df['stake_secondary'] + result_df['hedge_secondary']

    result_df['pnl_without_interest_primary'] = result_df['stake_secondary'] * result_df['close_price'] + \
        result_df['total_hedge_value_primary'] - \
        INITIAL_BALANCE_PRIMARY

    result_df['pnl_with_interest_primary'] = result_df['stake_secondary'] * result_df['close_price'] \
        + result_df['staking_reward_secondary'] * result_df['close_price'] \
        + result_df['funding_fee_primary'] \
        + result_df['total_hedge_value_primary'] \
        - INITIAL_BALANCE_PRIMARY

    return result_df

xdf = run_model(df, False)
xdf.to_csv('output.csv')
xdf
