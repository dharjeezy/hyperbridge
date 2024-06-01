import assert from "assert";
import { EventType, SupportedChain } from "../../../types";
import { GetRequestTimeoutHandledLog } from "../../../types/abi-interfaces/EthereumHostAbi";
import { getEvmChainFromTransaction } from "../../../utils/chain.helpers";
import { EvmHostEventsService } from "../../../services/evmHostEvents.service";

/**
 * Handles the GetRequestTimeoutHandled event
 */
export async function handleGetRequestTimeoutHandledEvent(
  event: GetRequestTimeoutHandledLog,
): Promise<void> {
  assert(event.args, "No handleGetRequestTimeoutHandledEvent args");

  const {
    args,
    block,
    transaction,
    transactionHash,
    transactionIndex,
    blockHash,
    blockNumber,
    data,
  } = event;
  const { commitment, dest } = args;

  logger.info(
    `Handling GetRequestTimeoutHandled Event: ${JSON.stringify({ blockNumber, transactionHash })}`,
  );

  const chain: SupportedChain = getEvmChainFromTransaction(transaction);

  await EvmHostEventsService.createEvent(
    {
      data,
      commitment,
      transactionHash,
      transactionIndex,
      blockHash,
      blockNumber,
      dest,
      timestamp: Number(block.timestamp),
      type: EventType.EVM_HOST_GET_REQUEST_TIMEOUT_HANDLED,
    },
    chain,
  );
}
