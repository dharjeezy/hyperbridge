import assert from "assert";
import { EventType, ResponseStatus, SupportedChain } from "../../../types";
import { PostResponseTimeoutHandledLog } from "../../../types/abi-interfaces/EthereumHostAbi";
import { getEvmChainFromTransaction } from "../../../utils/chain.helpers";
import { EvmHostEventsService } from "../../../services/evmHostEvents.service";
import { HyperBridgeService } from "../../../services/hyperbridge.service";
import { ResponseService } from "../../../services/response.service";

/**
 * Handles the PostResponseTimeoutHandled event
 */
export async function handlePostResponseTimeoutHandledEvent(
  event: PostResponseTimeoutHandledLog,
): Promise<void> {
  assert(event.args, "No handlePostResponseTimeoutHandledEvent args");

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
    `Handling PostResponseTimeoutHandled Event: ${JSON.stringify({ blockNumber, transactionHash })}`,
  );

  const chain: SupportedChain = getEvmChainFromTransaction(transaction);

  Promise.all([
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
        type: EventType.EVM_HOST_POST_RESPONSE_TIMEOUT_HANDLED,
      },
      chain,
    ),
    await HyperBridgeService.incrementNumberOfTimedOutMessagesSent(chain),
    await ResponseService.updateStatus({
      commitment,
      chain,
      blockNumber: blockNumber.toString(),
      blockHash: block.hash,
      blockTimestamp: block.timestamp,
      status: ResponseStatus.TIMED_OUT,
      transactionHash,
    }),
  ]);
}
