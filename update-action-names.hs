#!/usr/bin/env stack
{- stack script
  --package turtle
  --package text
  --package string-interpolate
  --resolver lts-19.10
-}
{-# LANGUAGE OverloadedStrings   #-}
{-# LANGUAGE QuasiQuotes         #-}
{-# LANGUAGE ScopedTypeVariables #-}

import Data.String.Interpolate
import System.Environment
import Turtle
import qualified Data.Text as T
import qualified Data.Text.IO as T


main = do
    let actionsFileName = "src/strategy.rs"
    actionsContent <- readTextFile actionsFileName
    scriptName <- getProgName
    let actionFunctionPrefix  = "pub async fn do_" :: Text
        actionFunctionHeaders = match (has (text actionFunctionPrefix <> plus (alphaNum <|> char '_') <> "(")) actionsContent
        actionFunctionNames   = map (T.dropEnd 1 . T.drop (T.length actionFunctionPrefix)) actionFunctionHeaders
        functionCalls         = T.intercalate "\n        " $ map (\fn -> format ("\"do_"%s%"\" => { d(); do_"%s%"(ctx).await }") fn fn) actionFunctionNames
        runFunctionPrefix     = "pub async fn run_action_by_name"::Text
        wholeRunFunction =
            [__i|#{runFunctionPrefix}(action_name: String, ctx: &mut Context) -> ActionResult {
                     // NOTE this function is autogenerated by "#{scriptName}" script
                     let d = || debug!("Run \\"{}\\"", action_name);
                     match action_name.as_str() {
                         #{functionCalls}
                         _ => err(format!("No such action \\"{}\\"", action_name)),
                     }
                 }|]
        (bodyBefore, bodyFuncAndAfter) = break (T.isPrefixOf [i|#{runFunctionPrefix}(|]) $ T.lines actionsContent
        (_, _:bodyAfter)               = break ("}" ==) bodyFuncAndAfter
        result                         = T.unlines bodyBefore <> wholeRunFunction <> T.unlines bodyAfter
    -- T.putStrLn wholeRunFunction
    writeTextFile actionsFileName result


