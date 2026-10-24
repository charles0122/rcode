package com.rcode.scanner

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/** Thin Android Canvas adapter. All RCode geometry comes from [RenderPlan]. */
object RCodeRenderer {
    fun draw(
        canvas: Canvas,
        bounds: RectF,
        plan: RenderPlan,
        foreground: Int,
        background: Int,
        logo: Bitmap? = null,
    ) {
        val side = min(bounds.width(), bounds.height())
        val left = bounds.left + (bounds.width() - side) / 2f
        val top = bounds.top + (bounds.height() - side) / 2f
        val scale = side / plan.viewport.toFloat()
        val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
        val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }

        canvas.drawRect(bounds, fill.apply { color = background })
        canvas.save()
        canvas.translate(left, top)
        canvas.scale(scale, scale)
        plan.commands.forEach { command ->
            val color = if (command.paint == DrawPaint.FOREGROUND) foreground else background
            val values = command.values
            when (command.kind) {
                DrawKind.CIRCLE -> canvas.drawCircle(
                    values[0].toFloat(),
                    values[1].toFloat(),
                    values[2].toFloat(),
                    fill.apply { this.color = color },
                )
                DrawKind.LINE -> {
                    stroke.color = color
                    stroke.strokeWidth = values[4].toFloat()
                    if (values[0] == values[2] && values[1] == values[3]) {
                        canvas.drawCircle(
                            values[0].toFloat(),
                            values[1].toFloat(),
                            values[4].toFloat() / 2f,
                            fill.apply { this.color = color },
                        )
                    } else {
                        canvas.drawLine(
                            values[0].toFloat(), values[1].toFloat(),
                            values[2].toFloat(), values[3].toFloat(), stroke,
                        )
                    }
                }
                DrawKind.ARC -> {
                    stroke.color = color
                    stroke.strokeWidth = values[5].toFloat()
                    val segments = ((values[2] * kotlin.math.abs(values[4]) / 1.5).toInt()).coerceAtLeast(2)
                    val path = Path()
                    for (index in 0..segments) {
                        val angle = values[3] + values[4] * index / segments
                        val x = values[0] + values[2] * cos(angle)
                        val y = values[1] + values[2] * sin(angle)
                        if (index == 0) path.moveTo(x.toFloat(), y.toFloat())
                        else path.lineTo(x.toFloat(), y.toFloat())
                    }
                    canvas.drawPath(path, stroke)
                }
            }
        }
        if (logo != null && plan.logoRadius > 0.0) {
            canvas.save()
            val path = Path().apply {
                addCircle(
                    plan.logoX.toFloat(),
                    plan.logoY.toFloat(),
                    plan.logoRadius.toFloat(),
                    Path.Direction.CW,
                )
            }
            canvas.clipPath(path)
            canvas.drawBitmap(
                logo,
                null,
                RectF(
                    (plan.logoX - plan.logoRadius).toFloat(),
                    (plan.logoY - plan.logoRadius).toFloat(),
                    (plan.logoX + plan.logoRadius).toFloat(),
                    (plan.logoY + plan.logoRadius).toFloat(),
                ),
                fill,
            )
            canvas.restore()
        }
        canvas.restore()
    }

    fun bitmap(
        plan: RenderPlan,
        size: Int,
        foreground: Int,
        background: Int,
        logo: Bitmap? = null,
    ): Bitmap {
        require(size > 0) { "size must be positive" }
        return Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888).also {
            draw(Canvas(it), RectF(0f, 0f, size.toFloat(), size.toFloat()), plan, foreground, background, logo)
        }
    }
}
