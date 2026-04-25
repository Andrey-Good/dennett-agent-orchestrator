import type { ErrorObject, ValidateFunction } from 'ajv'
import * as Ajv2020Module from 'ajv/dist/2020.js'
import type { JsonObject, JsonObjectSchema } from './json.js'

interface Ajv2020Like {
	addSchema(schema: unknown, key?: string): void
	compile<T>(schema: unknown): ValidateFunction<T>
	getSchema(key: string): ValidateFunction<unknown> | undefined
}

const Ajv2020 = Ajv2020Module.default as unknown as new (opts: {
	allErrors: boolean
	strict: boolean
	allowUnionTypes: boolean
}) => Ajv2020Like

const outputSchemaValidatorCache = new WeakMap<JsonObjectSchema, ValidateFunction<JsonObject>>()

export interface JsonOutputSchemaValidationResult {
	valid: boolean
	issues?: string[]
	message?: string
}

export function createAjv2020Validator(): Ajv2020Like {
	return new Ajv2020({
		allErrors: true,
		strict: false,
		allowUnionTypes: true,
	})
}

function formatAjvError(error: ErrorObject): string {
	const location = error.instancePath ? `${error.instancePath} ` : ''

	if (error.keyword === 'additionalProperties') {
		const additionalProperty = (error.params as Record<string, unknown>).additionalProperty
		if (typeof additionalProperty === 'string') {
			return `${location}must NOT have additional property "${additionalProperty}"`.trim()
		}
	}

	return `${location}${error.message ?? 'is invalid'}`.trim()
}

function getOutputSchemaValidator(schema: JsonObjectSchema): ValidateFunction<JsonObject> {
	const cachedValidator = outputSchemaValidatorCache.get(schema)
	if (cachedValidator) {
		return cachedValidator
	}

	const validator = createAjv2020Validator().compile<JsonObject>(schema as never)
	outputSchemaValidatorCache.set(schema, validator)
	return validator
}

export function validateJsonOutputAgainstSchema(
	schema: JsonObjectSchema,
	output: JsonObject,
): JsonOutputSchemaValidationResult {
	try {
		const validator = getOutputSchemaValidator(schema)
		if (validator(output)) {
			return {
				valid: true,
			}
		}

		const issues = (validator.errors ?? []).map((error) => formatAjvError(error))
		return {
			valid: false,
			issues,
			message:
				issues.length > 0
					? `JSON output failed schema validation: ${issues.join('; ')}`
					: 'JSON output failed schema validation.',
		}
	} catch (error) {
		const message = error instanceof Error ? error.message : 'Unknown schema compilation error.'
		return {
			valid: false,
			message: `JSON output schema could not be compiled: ${message}`,
		}
	}
}
